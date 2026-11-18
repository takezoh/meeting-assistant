use std::time::{Duration, Instant};

use crate::rules::RuleEvidence;
use crate::signals::BrowserTab;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectorEvent {
    Started(MeetingInfo),
    Ended(MeetingInfo),
}

/// A state machine that decides, for each meeting separately, when it starts and ends from the time series of
/// evidence. Several meetings can be in progress at once.
///
/// - Start: when a meeting's start condition holds for `start_debounce`.
/// - Continue: while that meeting's process keeps using the microphone (the window title does not matter).
/// - End: when the continue condition lapses for `end_grace`.
///
/// The caller supplies the time. It has no side effects.
#[derive(Debug)]
pub struct MeetingDetector {
    start_debounce: Duration,
    end_grace: Duration,
    /// Meetings in progress, in the order they started.
    meetings: Vec<Tracked>,
    /// Meetings whose start condition holds but not yet for `start_debounce`, with the time it began to hold.
    candidates: Vec<(MeetingKey, Instant)>,
}

#[derive(Debug)]
struct Tracked {
    info: MeetingInfo,
    absent_since: Option<Instant>,
}

impl MeetingDetector {
    pub fn new(start_debounce: Duration, end_grace: Duration) -> Self {
        Self {
            start_debounce,
            end_grace,
            meetings: Vec::new(),
            candidates: Vec::new(),
        }
    }

    /// Meetings in progress, in the order they started.
    pub fn meetings(&self) -> impl Iterator<Item = &MeetingInfo> {
        self.meetings.iter().map(|tracked| &tracked.info)
    }

    /// Advances every meeting by one observation. Ends are reported before starts.
    pub fn update(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Vec<DetectorEvent> {
        let mut events = self.update_meetings(now, evidence);
        events.extend(self.update_candidates(now, evidence));
        events
    }

    fn update_meetings(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Vec<DetectorEvent> {
        let mut ended = Vec::new();
        let end_grace = self.end_grace;
        self.meetings.retain_mut(|tracked| {
            let key = tracked.info.key();
            if evidence.iter().any(|e| e.mic_active && e.key() == key) {
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
        for found in evidence.iter().filter(|e| e.can_start) {
            let key = found.key();
            let in_progress = self.meetings.iter().any(|tracked| tracked.info.key() == key);
            let already_seen = self.candidates.iter().any(|(candidate, _)| *candidate == key);
            if in_progress || already_seen {
                continue;
            }
            // Keep the time the condition began to hold while it keeps holding.
            let since = previous
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map_or(now, |(_, since)| *since);
            if now.duration_since(since) < self.start_debounce {
                self.candidates.push((key, since));
                continue;
            }
            let meeting = MeetingInfo {
                rule_name: found.rule_name.clone(),
                capture_device_id: found.capture_device_id.clone(),
                render_device_id: found.render_device_id.clone(),
                process_id: found.process_id,
                window: found.window,
                display: found.display.clone(),
                browser_tab: found.browser_tab,
                title: found.title.clone(),
            };
            self.meetings.push(Tracked {
                info: meeting.clone(),
                absent_since: None,
            });
            started.push(DetectorEvent::Started(meeting));
        }
        started
    }
}
