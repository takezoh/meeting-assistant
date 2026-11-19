use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::apps::WindowRole;
use crate::rules::{AppWindow, RuleEvidence};
use crate::signals::BrowserTab;

/// A window that appeared this long before an app began using the microphone, or later, is taken to be the window
/// of the meeting that followed: apps open the call's window as the call starts.
pub const MEETING_WINDOW_LOOKBACK: Duration = Duration::from_secs(10);

/// Identifies one meeting from tick to tick: the rule that detected it and, for a meeting reported by the browser
/// extension, its tab. Two tabs of the same service are two meetings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeetingKey {
    pub rule_name: String,
    pub browser_tab: Option<BrowserTab>,
}

/// A detected meeting. The devices and display are values observed at start time, used to decide what to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingInfo {
    pub rule_name: String,
    pub capture_device_id: Option<String>,
    pub render_device_id: Option<String>,
    pub process_id: Option<u32>,
    pub window: Option<u64>,
    pub display: Option<String>,
    pub browser_tab: Option<BrowserTab>,
    /// What tells the meeting apart from others of the same rule, as observed at start. Shown to the user only.
    pub title: Option<String>,
}

impl MeetingInfo {
    pub fn key(&self) -> MeetingKey {
        MeetingKey {
            rule_name: self.rule_name.clone(),
            browser_tab: self.browser_tab,
        }
    }
}

/// How long a meeting must be active before it starts, and inactive before it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub start_debounce: Duration,
    pub end_grace: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectorEvent {
    Started(MeetingInfo),
    Ended(MeetingInfo),
}

/// A state machine that decides, for each meeting separately, when it starts and ends from the time series of
/// evidence. Several meetings can be in progress at once.
///
/// - Start: when a meeting has been active (its app uses the microphone, or the extension reports a call) for the
///   start debounce.
/// - Its window: a window that the app's adapter (see [`crate::MeetingApp`]) tells is a meeting's, or else, of the
///   windows nothing is told about, one that appeared once the windows were being read, from
///   [`MEETING_WINDOW_LOOKBACK`] before the meeting's condition began. It is chosen by the start or later during the
///   meeting. A window the adapter tells is the app's main window, and an untold window already open when reading
///   began, are never the meeting's. Until it has one, a meeting has no window and nothing is recorded for its
///   picture.
/// - Continue: while it stays active, as it started. Its window is for the picture only: Slack hides its huddle window
///   when the huddle is left and destroys it seconds later, and a window cannot tell that apart from a meeting.
/// - End: when it has not been active for the end grace.
///
/// Desktop apps and browser meetings have their own [`Timing`].
///
/// The caller supplies the time and says whether the windows were read. It has no side effects.
#[derive(Debug)]
pub struct MeetingDetector {
    apps: Timing,
    browser: Timing,
    /// Meetings in progress, in the order they started.
    meetings: Vec<Tracked>,
    /// Meetings that are active but not yet for their start debounce, with the time they became active.
    candidates: Vec<(MeetingKey, Instant)>,
    /// When each window of a rule's app appeared while the windows have been read without a break; `None` for windows
    /// already open when reading began, which cannot be told to have appeared with a meeting.
    windows_seen: HashMap<u64, Option<Instant>>,
    /// Whether the windows were read at the previous update.
    reading: bool,
}

#[derive(Debug)]
struct Tracked {
    info: MeetingInfo,
    /// When its condition began to hold.
    since: Instant,
    /// The meeting's window, while it is open.
    window: Option<AppWindow>,
    absent_since: Option<Instant>,
}

impl MeetingDetector {
    /// `apps` applies to meetings detected from a desktop app, `browser` to meetings the extension reports.
    pub fn new(apps: Timing, browser: Timing) -> Self {
        Self {
            apps,
            browser,
            meetings: Vec::new(),
            candidates: Vec::new(),
            windows_seen: HashMap::new(),
            reading: false,
        }
    }

    /// Whether the meeting with `key` is going on now: its condition holds.
    pub fn is_ongoing(&self, key: &MeetingKey, evidence: &[RuleEvidence]) -> bool {
        self.meetings
            .iter()
            .find(|tracked| tracked.info.key() == *key)
            .is_some_and(|tracked| ongoing(tracked, evidence))
    }

    /// The window of the meeting with `key`, once it has one and while it is open.
    pub fn meeting_window(&self, key: &MeetingKey) -> Option<u64> {
        let tracked = self.meetings.iter().find(|tracked| tracked.info.key() == *key)?;
        tracked.window.as_ref().map(|window| window.id)
    }

    /// Meetings in progress, in the order they started.
    pub fn meetings(&self) -> impl Iterator<Item = &MeetingInfo> {
        self.meetings.iter().map(|tracked| &tracked.info)
    }

    /// Advances every meeting by one observation. `windows_read` says whether the evidence carries the apps' windows;
    /// without them, what is known of the windows stays as it was. Ends are reported before starts.
    pub fn update(&mut self, now: Instant, evidence: &[RuleEvidence], windows_read: bool) -> Vec<DetectorEvent> {
        if windows_read {
            self.see_windows(now, evidence);
        } else {
            self.reading = false;
        }
        let mut events = self.update_meetings(now, evidence, windows_read);
        events.extend(self.update_candidates(now, evidence));
        events
    }

    /// Notes when each window appeared, and forgets windows that are gone. When reading resumes, every window found
    /// counts as having been open before.
    fn see_windows(&mut self, now: Instant, evidence: &[RuleEvidence]) {
        let resumed = !std::mem::replace(&mut self.reading, true);
        let current: HashSet<u64> = evidence.iter().flat_map(|e| e.windows.iter().map(|w| w.id)).collect();
        if resumed {
            self.windows_seen = current.into_iter().map(|id| (id, None)).collect();
            return;
        }
        self.windows_seen.retain(|id, _| current.contains(id));
        for id in current {
            self.windows_seen.entry(id).or_insert(Some(now));
        }
    }

    fn update_meetings(&mut self, now: Instant, evidence: &[RuleEvidence], windows_read: bool) -> Vec<DetectorEvent> {
        let mut ended = Vec::new();
        let (apps, browser) = (self.apps, self.browser);
        let seen = &self.windows_seen;
        self.meetings.retain_mut(|tracked| {
            let key = tracked.info.key();
            let end_grace = timing_of(&key, apps, browser).end_grace;
            if windows_read && key.browser_tab.is_none() {
                let windows = evidence
                    .iter()
                    .find(|e| e.key() == key)
                    .map_or(&[][..], |e| e.windows.as_slice());
                // A closed window is let go; another that appears afterwards can take its place.
                if tracked
                    .window
                    .as_ref()
                    .is_some_and(|window| !windows.iter().any(|w| w.id == window.id))
                {
                    tracked.window = None;
                }
                if tracked.window.is_none() {
                    tracked.window = meeting_window_among(seen, windows, tracked.since).cloned();
                }
            }
            if ongoing(tracked, evidence) {
                tracked.absent_since = None;
                return true;
            }
            let absent_since = *tracked.absent_since.get_or_insert(now);
            if now.duration_since(absent_since) < end_grace {
                return true;
            }
            ended.push(DetectorEvent::Ended(tracked.info.clone()));
            false
        });
        ended
    }

    fn update_candidates(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Vec<DetectorEvent> {
        let previous = std::mem::take(&mut self.candidates);
        let mut started = Vec::new();
        for found in evidence.iter().filter(|e| e.active) {
            let key = found.key();
            let in_progress = self.meetings.iter().any(|tracked| tracked.info.key() == key);
            let already_seen = self.candidates.iter().any(|(candidate, _)| *candidate == key);
            if in_progress || already_seen {
                continue;
            }
            // Keep the time it became active while it stays active.
            let since = previous
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map_or(now, |(_, since)| *since);
            if now.duration_since(since) < timing_of(&key, self.apps, self.browser).start_debounce {
                self.candidates.push((key, since));
                continue;
            }
            let window = meeting_window_among(&self.windows_seen, &found.windows, since).cloned();
            let meeting = MeetingInfo {
                rule_name: found.rule_name.clone(),
                capture_device_id: found.capture_device_id.clone(),
                render_device_id: found.render_device_id.clone(),
                process_id: found.process_id,
                window: window.as_ref().map(|w| w.id),
                display: window.as_ref().and_then(|w| w.display.clone()),
                browser_tab: found.browser_tab,
                title: found.title.clone(),
            };
            self.meetings.push(Tracked {
                info: meeting.clone(),
                since,
                window,
                absent_since: None,
            });
            started.push(DetectorEvent::Started(meeting));
        }
        started
    }
}

/// The window of a meeting whose condition began at `since`, the largest of the app's windows that can be its: those
/// the app's adapter tells are a meeting's, and of the others it tells nothing about, those that appeared from
/// [`MEETING_WINDOW_LOOKBACK`] before then.
fn meeting_window_among<'a>(
    seen: &HashMap<u64, Option<Instant>>,
    windows: &'a [AppWindow],
    since: Instant,
) -> Option<&'a AppWindow> {
    let from = since.checked_sub(MEETING_WINDOW_LOOKBACK).unwrap_or(since);
    windows
        .iter()
        .filter(|w| match w.role {
            WindowRole::Meeting => true,
            WindowRole::Main => false,
            WindowRole::Unknown => matches!(seen.get(&w.id), Some(Some(appeared)) if *appeared >= from),
        })
        .max_by_key(|w| w.area)
}

/// Whether a tracked meeting's condition holds.
fn ongoing(tracked: &Tracked, evidence: &[RuleEvidence]) -> bool {
    let key = tracked.info.key();
    evidence.iter().any(|e| e.key() == key && e.active)
}

/// Browser meetings are the ones the extension reports, which carry their tab.
fn timing_of(key: &MeetingKey, apps: Timing, browser: Timing) -> Timing {
    if key.browser_tab.is_some() { browser } else { apps }
}
