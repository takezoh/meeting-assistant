use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

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
///   start debounce. A desktop app's meeting is given the window that appeared with it (see
///   [`MEETING_WINDOW_LOOKBACK`]), or else the app's largest window.
/// - Continue: while it stays active, or while the window that appeared with it stays open (an app may let go of the
///   microphone while muted, but its call window stays).
/// - End: when it has been neither for the end grace.
///
/// Desktop apps and browser meetings have their own [`Timing`].
///
/// The caller supplies the time. It has no side effects.
#[derive(Debug)]
pub struct MeetingDetector {
    apps: Timing,
    browser: Timing,
    /// Meetings in progress, in the order they started.
    meetings: Vec<Tracked>,
    /// Meetings that are active but not yet for their start debounce, with the time they became active.
    candidates: Vec<(MeetingKey, Instant)>,
    /// When each window of a rule's app was first seen; `None` for windows already there at the first update, which
    /// cannot have appeared with a meeting.
    windows_seen: HashMap<u64, Option<Instant>>,
    updated: bool,
}

#[derive(Debug)]
struct Tracked {
    info: MeetingInfo,
    /// The window that appeared with the meeting, which keeps it going while it is open.
    meeting_window: Option<u64>,
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
            updated: false,
        }
    }

    /// Whether the meeting with `key` is going on now: its condition holds, or the window that appeared with it is
    /// still open.
    pub fn is_ongoing(&self, key: &MeetingKey, evidence: &[RuleEvidence]) -> bool {
        self.meetings
            .iter()
            .find(|tracked| tracked.info.key() == *key)
            .is_some_and(|tracked| ongoing(tracked, evidence))
    }

    /// Meetings in progress, in the order they started.
    pub fn meetings(&self) -> impl Iterator<Item = &MeetingInfo> {
        self.meetings.iter().map(|tracked| &tracked.info)
    }

    /// Advances every meeting by one observation. Ends are reported before starts.
    pub fn update(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Vec<DetectorEvent> {
        self.see_windows(now, evidence);
        let mut events = self.update_meetings(now, evidence);
        events.extend(self.update_candidates(now, evidence));
        events
    }

    /// Notes when each window was first seen, and forgets windows that are gone.
    fn see_windows(&mut self, now: Instant, evidence: &[RuleEvidence]) {
        let first = !std::mem::replace(&mut self.updated, true);
        let current: HashSet<u64> = evidence.iter().flat_map(|e| e.windows.iter().map(|w| w.id)).collect();
        self.windows_seen.retain(|id, _| current.contains(id));
        for id in current {
            self.windows_seen.entry(id).or_insert((!first).then_some(now));
        }
    }

    /// The window that appeared with a meeting whose condition began at `since`: the largest of the app's windows
    /// first seen from [`MEETING_WINDOW_LOOKBACK`] before then.
    fn meeting_window<'a>(&self, windows: &'a [AppWindow], since: Instant) -> Option<&'a AppWindow> {
        let from = since.checked_sub(MEETING_WINDOW_LOOKBACK).unwrap_or(since);
        windows
            .iter()
            .filter(|w| matches!(self.windows_seen.get(&w.id), Some(Some(seen)) if *seen >= from))
            .max_by_key(|w| w.area)
    }

    fn update_meetings(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Vec<DetectorEvent> {
        let mut ended = Vec::new();
        let (apps, browser) = (self.apps, self.browser);
        self.meetings.retain_mut(|tracked| {
            let end_grace = timing_of(&tracked.info.key(), apps, browser).end_grace;
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
            let appeared = self.meeting_window(&found.windows, since);
            let meeting = MeetingInfo {
                rule_name: found.rule_name.clone(),
                capture_device_id: found.capture_device_id.clone(),
                render_device_id: found.render_device_id.clone(),
                process_id: found.process_id,
                window: appeared.map(|w| w.id).or(found.window),
                display: appeared.map_or_else(|| found.display.clone(), |w| w.display.clone()),
                browser_tab: found.browser_tab,
                title: found.title.clone(),
            };
            self.meetings.push(Tracked {
                info: meeting.clone(),
                meeting_window: appeared.map(|w| w.id),
                absent_since: None,
            });
            started.push(DetectorEvent::Started(meeting));
        }
        started
    }
}

/// Whether a tracked meeting's condition holds, or the window that appeared with it is still open.
fn ongoing(tracked: &Tracked, evidence: &[RuleEvidence]) -> bool {
    let key = tracked.info.key();
    evidence.iter().filter(|e| e.key() == key).any(|e| {
        e.active
            || tracked
                .meeting_window
                .is_some_and(|window| e.windows.iter().any(|w| w.id == window))
    })
}

/// Browser meetings are the ones the extension reports, which carry their tab.
fn timing_of(key: &MeetingKey, apps: Timing, browser: Timing) -> Timing {
    if key.browser_tab.is_some() { browser } else { apps }
}
