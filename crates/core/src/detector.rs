use std::time::{Duration, Instant};

use crate::rules::RuleEvidence;
use crate::signals::BrowserTab;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectorEvent {
    Started(MeetingInfo),
    Ended(MeetingInfo),
}

/// A state machine that decides meeting start and end from each rule's time series of evidence.
///
/// - Start: when some rule's start condition holds for `start_debounce`.
/// - Continue: while the started rule's process keeps using the microphone (the window title does not matter).
/// - End: when the continue condition lapses for `end_grace`.
///
/// The caller supplies the time. It has no side effects.
#[derive(Debug)]
pub struct MeetingDetector {
    start_debounce: Duration,
    end_grace: Duration,
    current: Option<MeetingInfo>,
    candidate: Option<(String, Instant)>,
    absent_since: Option<Instant>,
}

impl MeetingDetector {
    pub fn new(start_debounce: Duration, end_grace: Duration) -> Self {
        Self {
            start_debounce,
            end_grace,
            current: None,
            candidate: None,
            absent_since: None,
        }
    }

    pub fn current(&self) -> Option<&MeetingInfo> {
        self.current.as_ref()
    }

    pub fn update(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Option<DetectorEvent> {
        match self.current.take() {
            None => self.update_idle(now, evidence),
            Some(meeting) => self.update_in_meeting(now, evidence, meeting),
        }
    }

    fn update_idle(&mut self, now: Instant, evidence: &[RuleEvidence]) -> Option<DetectorEvent> {
        // Keep the candidate rule while it still meets its condition, so the candidate never switches mid-decision.
        let candidate_rule = self.candidate.as_ref().map(|(rule, _)| rule.as_str());
        let Some(found) = evidence
            .iter()
            .find(|e| e.can_start && Some(e.rule_name.as_str()) == candidate_rule)
            .or_else(|| evidence.iter().find(|e| e.can_start))
        else {
            self.candidate = None;
            return None;
        };

        let since = match &self.candidate {
            Some((rule, since)) if *rule == found.rule_name => *since,
            _ => {
                self.candidate = Some((found.rule_name.clone(), now));
                now
            }
        };
        if now.duration_since(since) < self.start_debounce {
            return None;
        }

        self.candidate = None;
        self.absent_since = None;
        let meeting = MeetingInfo {
            rule_name: found.rule_name.clone(),
            capture_device_id: found.capture_device_id.clone(),
            render_device_id: found.render_device_id.clone(),
            process_id: found.process_id,
            window: found.window,
            display: found.display.clone(),
            browser_tab: found.browser_tab,
        };
        self.current = Some(meeting.clone());
        Some(DetectorEvent::Started(meeting))
    }

    fn update_in_meeting(
        &mut self,
        now: Instant,
        evidence: &[RuleEvidence],
        meeting: MeetingInfo,
    ) -> Option<DetectorEvent> {
        if evidence
            .iter()
            .any(|e| e.rule_name == meeting.rule_name && e.mic_active)
        {
            self.absent_since = None;
            self.current = Some(meeting);
            return None;
        }

        let absent_since = *self.absent_since.get_or_insert(now);
        if now.duration_since(absent_since) < self.end_grace {
            self.current = Some(meeting);
            return None;
        }

        self.absent_since = None;
        Some(DetectorEvent::Ended(meeting))
    }
}
