use std::collections::{HashMap, HashSet};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::config::ConfigError;
use crate::signals::{BrowserTab, ProcessEntry, SignalSnapshot};

/// A detection rule: "if this process is using the microphone, a meeting is in progress".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetingRule {
    /// Rule name. Used in recording file names and logs.
    pub name: String,
    /// Target executable name (extension and letter case are ignored). Descendant processes are also covered.
    pub process_names: Vec<String>,
    /// If true, the start check additionally requires that "a window title of the target process matches one of
    /// window_title_patterns". For processes such as browsers, where microphone use alone does not imply a
    /// meeting.
    #[serde(default)]
    pub require_window_title: bool,
    /// Regular expressions for window titles (case-insensitive).
    #[serde(default)]
    pub window_title_patterns: Vec<String>,
}

/// Default rules. Process names are for Windows (split per OS when supporting other OSes).
pub fn default_rules() -> Vec<MeetingRule> {
    fn app(name: &str, process_names: &[&str]) -> MeetingRule {
        MeetingRule {
            name: name.into(),
            process_names: process_names.iter().map(|s| s.to_string()).collect(),
            require_window_title: false,
            window_title_patterns: Vec::new(),
        }
    }

    vec![
        app("Zoom", &["zoom"]),
        app("Teams", &["ms-teams", "teams"]),
        app("Webex", &["webexmta", "atmgr", "ciscocollabhost", "webex"]),
        app("Slack", &["slack"]),
        MeetingRule {
            require_window_title: true,
            window_title_patterns: [
                r"^(\(\d+\)\s*)?Meet\s*[-–—:]\s",
                r"\bGoogle Meet\b",
                r"\bMicrosoft Teams\b",
                r"\bZoom\b",
                r"\bWebex\b",
                r"\bSlack\b",
                r"\bWhereby\b",
                r"\bJitsi Meet\b",
            ]
            .map(String::from)
            .to_vec(),
            ..app(
                "WebMeeting",
                &["chrome", "msedge", "firefox", "brave", "vivaldi", "opera", "arc"],
            )
        },
    ]
}

/// Evidence read from the signals at a point in time for one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvidence {
    pub rule_name: String,
    /// A process of the rule's target (including descendants) is using the microphone.
    pub mic_active: bool,
    /// A window title of the target process matches a pattern.
    pub window_title_matched: bool,
    /// The meeting start condition is met.
    pub can_start: bool,
    pub capture_device_id: Option<String>,
    pub render_device_id: Option<String>,
    /// Top-level process of the app that is using the microphone. Used to record only that app's sound.
    pub process_id: Option<u32>,
    /// The window considered to be showing the meeting.
    pub window: Option<u64>,
    /// The display that holds that window.
    pub display: Option<String>,
    /// For a meeting reported by the browser extension, that tab.
    pub browser_tab: Option<BrowserTab>,
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
    require_window_title: bool,
    title_patterns: Vec<Regex>,
}

const MAX_ANCESTOR_DEPTH: usize = 32;

impl RuleEvaluator {
    pub fn new(rules: &[MeetingRule]) -> Result<Self, ConfigError> {
        let rules = rules.iter().map(compile).collect::<Result<Vec<_>, _>>()?;
        let known_names = rules.iter().flat_map(|r| r.process_names.iter().cloned()).collect();
        Ok(Self { rules, known_names })
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
                let windows: Vec<_> = snapshot
                    .windows
                    .iter()
                    .filter(|w| owned(w.process_id).is_some())
                    .collect();
                let matched: Vec<_> = windows
                    .iter()
                    .copied()
                    .filter(|w| rule.title_patterns.iter().any(|p| p.is_match(&w.title)))
                    .collect();

                let mic_active = capture.is_some();
                let window_title_matched = !matched.is_empty();
                // Record the window whose title matched, or else the largest window.
                let window = if window_title_matched { &matched } else { &windows }
                    .iter()
                    .max_by_key(|w| w.area);

                RuleEvidence {
                    rule_name: rule.name.clone(),
                    mic_active,
                    window_title_matched,
                    can_start: mic_active && (!rule.require_window_title || window_title_matched),
                    capture_device_id: capture.map(|(s, _)| s.device_id.clone()),
                    render_device_id: render.map(|s| s.device_id.clone()),
                    process_id: capture.map(|(_, o)| o.root_process_id),
                    window: window.map(|w| w.id),
                    display: window.and_then(|w| w.display.clone()),
                    browser_tab: None,
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
                evidence.mic_active = false;
                evidence.can_start = false;
            }
        }

        // A meeting reported by the extension meets the start condition by itself. It takes priority over rules.
        let mut result: Vec<RuleEvidence> = snapshot
            .browser_meetings
            .iter()
            .map(|meeting| RuleEvidence {
                rule_name: meeting.service.clone(),
                mic_active: true,
                window_title_matched: false,
                can_start: true,
                capture_device_id: None,
                render_device_id: None,
                process_id: None,
                window: None,
                display: None,
                browser_tab: Some(meeting.tab),
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
    if rule.require_window_title && rule.window_title_patterns.is_empty() {
        return Err(invalid(
            "require_window_title is true but window_title_patterns is missing".into(),
        ));
    }

    let title_patterns = rule
        .window_title_patterns
        .iter()
        .map(|pattern| {
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|e| invalid(format!("regex '{pattern}' is invalid: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CompiledRule {
        name: rule.name.clone(),
        process_names: rule.process_names.iter().map(|n| normalize_process_name(n)).collect(),
        require_window_title: rule.require_window_title,
        title_patterns,
    })
}
