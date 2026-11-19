use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::apps::{MeetingApp, WindowRole};
use crate::config::ConfigError;
use crate::detector::MeetingKey;
use crate::signals::{BrowserTab, ProcessEntry, SignalSnapshot};

/// A detection rule: "if this process is using the microphone, a meeting is in progress".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetingRule {
    /// Rule name. Used in recording file names and logs.
    pub name: String,
    /// Target executable name (extension and letter case are ignored). Descendant processes are also covered.
    pub process_names: Vec<String>,
}

/// Default rules: desktop meeting apps, with their executable names on Windows and on macOS (`zoom.us`, `MSTeams`).
/// Browser meetings are detected by the browser extension, not by rules.
pub fn default_rules() -> Vec<MeetingRule> {
    fn app(name: &str, process_names: &[&str]) -> MeetingRule {
        MeetingRule {
            name: name.into(),
            process_names: process_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    vec![
        app("Zoom", &["zoom", "zoom.us"]),
        app("Teams", &["ms-teams", "teams", "msteams"]),
        app("Webex", &["webexmta", "atmgr", "ciscocollabhost", "webex"]),
        app("Slack", &["slack"]),
    ]
}

/// Evidence read from the signals at a point in time for one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvidence {
    pub rule_name: String,
    /// The meeting's condition holds now: a process of the rule's target (including descendants) is using the
    /// microphone, or the extension reports a connected call.
    pub active: bool,
    pub capture_device_id: Option<String>,
    pub render_device_id: Option<String>,
    /// Top-level process of the app that is using the microphone. Used to record only that app's sound.
    pub process_id: Option<u32>,
    /// Every window of the app, when the windows were read, from which the detector tells which one is the meeting's.
    /// The app's other windows, its main window among them, are never recorded.
    pub windows: Vec<AppWindow>,
    /// For a meeting reported by the browser extension, that tab.
    pub browser_tab: Option<BrowserTab>,
    /// What tells the meeting apart from others of the same rule, when known. Shown to the user only.
    pub title: Option<String>,
}

/// A window of the app a rule names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppWindow {
    pub id: u64,
    pub area: u64,
    pub display: Option<String>,
    /// What the app's adapter tells of it; [`WindowRole::Unknown`] for an app without one.
    pub role: WindowRole,
}

impl RuleEvidence {
    /// The meeting this evidence is about.
    pub fn key(&self) -> MeetingKey {
        MeetingKey {
            rule_name: self.rule_name.clone(),
            browser_tab: self.browser_tab,
        }
    }
}

/// Converts a signal snapshot into per-rule evidence. Holds no state.
#[derive(Debug)]
pub struct RuleEvaluator {
    rules: Vec<CompiledRule>,
    known_names: HashSet<String>,
}

#[derive(Debug)]
struct CompiledRule {
    name: String,
    process_names: HashSet<String>,
    /// The adapter for the rule's app, if there is one.
    app: Option<Arc<dyn MeetingApp>>,
}

const MAX_ANCESTOR_DEPTH: usize = 32;

impl RuleEvaluator {
    pub fn new(rules: &[MeetingRule]) -> Result<Self, ConfigError> {
        let rules = rules.iter().map(compile).collect::<Result<Vec<_>, _>>()?;
        let known_names = rules.iter().flat_map(|r| r.process_names.iter().cloned()).collect();
        Ok(Self { rules, known_names })
    }

    /// Gives each rule the first of `apps` that handles one of its process names.
    pub fn with_apps(mut self, apps: &[Arc<dyn MeetingApp>]) -> Self {
        for rule in &mut self.rules {
            rule.app = apps
                .iter()
                .find(|app| rule.process_names.iter().any(|name| app.handles(name)))
                .cloned();
        }
        self
    }

    pub fn evaluate(&self, snapshot: &SignalSnapshot) -> Vec<RuleEvidence> {
        let processes: HashMap<u32, &ProcessEntry> = snapshot.processes.iter().map(|p| (p.process_id, p)).collect();
        let mut owners: HashMap<u32, Option<Owner>> = HashMap::new();
        let mut owner = |pid: u32| *owners.entry(pid).or_insert_with(|| self.find_owner(pid, &processes));

        let mut by_rule: Vec<RuleEvidence> = self
            .rules
            .iter()
            .map(|rule| {
                let mut owned = |pid: u32| owner(pid).filter(|o| rule.process_names.contains(o.name));

                let capture = snapshot
                    .capture_sessions
                    .iter()
                    .find_map(|s| owned(s.process_id).map(|o| (s, o)));
                let render = snapshot.render_sessions.iter().find(|s| owned(s.process_id).is_some());
                let entries: Vec<_> = snapshot
                    .windows
                    .iter()
                    .filter(|w| owned(w.process_id).is_some())
                    .collect();
                let roles = rule.app.as_ref().map_or_else(
                    || vec![WindowRole::Unknown; entries.len()],
                    |app| app.window_roles(&entries.iter().map(|w| w.title.as_str()).collect::<Vec<_>>()),
                );
                let windows: Vec<AppWindow> = entries
                    .iter()
                    .zip(roles.into_iter().chain(std::iter::repeat(WindowRole::Unknown)))
                    .map(|(w, role)| AppWindow {
                        id: w.id,
                        area: w.area,
                        display: w.display.clone(),
                        role,
                    })
                    .collect();

                RuleEvidence {
                    rule_name: rule.name.clone(),
                    active: capture.is_some(),
                    capture_device_id: capture.map(|(s, _)| s.device_id.clone()),
                    render_device_id: render.map(|s| s.device_id.clone()),
                    process_id: capture.map(|(_, o)| o.root_process_id),
                    windows,
                    browser_tab: None,
                    title: None,
                }
            })
            .collect();

        // A browser connected to the extension trusts only the extension's reports (avoids double detection).
        let linked: HashSet<u32> = snapshot
            .browser_links
            .iter()
            .filter_map(|link| self.find_owner(link.host_process_id, &processes))
            .map(|owner| owner.root_process_id)
            .collect();
        for evidence in &mut by_rule {
            if evidence.process_id.is_some_and(|pid| linked.contains(&pid)) {
                evidence.active = false;
            }
        }

        // A meeting reported by the extension is active by itself. It takes priority over rules.
        let mut result: Vec<RuleEvidence> = snapshot
            .browser_meetings
            .iter()
            .map(|meeting| RuleEvidence {
                rule_name: meeting.service.clone(),
                active: true,
                capture_device_id: None,
                render_device_id: None,
                process_id: None,
                windows: Vec::new(),
                browser_tab: Some(meeting.tab),
                title: meeting.title.clone(),
            })
            .collect();
        result.append(&mut by_rule);
        result
    }

    /// Walks up parents from the pid to find the nearest process that any rule targets.
    /// Catches cases where a child of the target app handles audio, such as WebView2 or a browser audio service.
    ///
    /// Once found, keeps going up while parents have the same name to also get the app's top-level process
    /// (browsers have chains of same-named processes).
    fn find_owner(&self, mut pid: u32, processes: &HashMap<u32, &ProcessEntry>) -> Option<Owner<'_>> {
        let mut visited = HashSet::new();
        let mut owner: Option<Owner> = None;
        for _ in 0..MAX_ANCESTOR_DEPTH {
            if !visited.insert(pid) {
                break;
            }
            let Some(entry) = processes.get(&pid) else { break };
            let name = normalize_process_name(&entry.name);
            match &mut owner {
                None => {
                    if let Some(known) = self.known_names.get(&name) {
                        owner = Some(Owner {
                            name: known,
                            root_process_id: pid,
                        });
                    }
                }
                Some(found) if found.name == name => found.root_process_id = pid,
                Some(_) => break,
            }
            if entry.parent_process_id == 0 {
                break;
            }
            pid = entry.parent_process_id;
        }
        owner
    }
}

/// The app that owns a process.
#[derive(Debug, Clone, Copy)]
struct Owner<'a> {
    /// The process name the rule targets (normalized for comparison).
    name: &'a str,
    root_process_id: u32,
}

/// A process name for comparison. Trims surrounding whitespace and `.exe`, and lowercases it.
fn normalize_process_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    lower.strip_suffix(".exe").map(str::to_owned).unwrap_or(lower)
}

fn compile(rule: &MeetingRule) -> Result<CompiledRule, ConfigError> {
    let invalid = |reason: String| ConfigError::InvalidRule {
        rule: rule.name.clone(),
        reason,
    };

    if rule.name.trim().is_empty() {
        return Err(ConfigError::Invalid("rule has no name".into()));
    }
    if rule.process_names.is_empty() {
        return Err(invalid("process_names is missing".into()));
    }

    Ok(CompiledRule {
        name: rule.name.clone(),
        process_names: rule.process_names.iter().map(|n| normalize_process_name(n)).collect(),
        app: None,
    })
}
