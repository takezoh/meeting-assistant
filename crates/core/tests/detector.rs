use std::time::{Duration, Instant};

use meeting_assistant_core::{DetectorEvent, MeetingDetector, RuleEvidence};

struct Clock(Instant);

impl Clock {
    fn at(&self, seconds: f64) -> Instant {
        self.0 + Duration::from_secs_f64(seconds)
    }
}

fn detector() -> (MeetingDetector, Clock) {
    (
        MeetingDetector::new(Duration::from_secs(3), Duration::from_secs(15)),
        Clock(Instant::now()),
    )
}

fn start(rule: &str) -> Vec<RuleEvidence> {
    vec![RuleEvidence {
        rule_name: rule.into(),
        mic_active: true,
        window_title_matched: true,
        can_start: true,
        capture_device_id: Some("mic".into()),
        render_device_id: Some("speaker".into()),
        process_id: Some(100),
        window: Some(77),
        display: Some("DISPLAY2".into()),
        browser_tab: None,
    }]
}

fn mic_only(rule: &str) -> Vec<RuleEvidence> {
    vec![RuleEvidence {
        rule_name: rule.into(),
        mic_active: true,
        window_title_matched: false,
        can_start: false,
        capture_device_id: Some("mic".into()),
        render_device_id: None,
        process_id: Some(100),
        window: None,
        display: None,
        browser_tab: None,
    }]
}

const NOTHING: &[RuleEvidence] = &[];

fn started_rule(event: Option<DetectorEvent>) -> String {
    match event {
        Some(DetectorEvent::Started(meeting)) => meeting.rule_name,
        other => panic!("not a start event: {other:?}"),
    }
}

fn ended_rule(event: Option<DetectorEvent>) -> String {
    match event {
        Some(DetectorEvent::Ended(meeting)) => meeting.rule_name,
        other => panic!("not an end event: {other:?}"),
    }
}

#[test]
fn does_not_start_until_the_start_condition_lasts_for_the_grace_period() {
    let (mut d, t) = detector();
    assert_eq!(d.update(t.at(0.0), &start("Zoom")), None);
    assert_eq!(d.update(t.at(2.9), &start("Zoom")), None);
    assert_eq!(started_rule(d.update(t.at(3.0), &start("Zoom"))), "Zoom");
}

#[test]
fn a_started_meeting_has_the_observed_device_window_and_process() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    let Some(DetectorEvent::Started(meeting)) = d.update(t.at(3.0), &start("Zoom")) else {
        panic!()
    };
    assert_eq!(meeting.capture_device_id.as_deref(), Some("mic"));
    assert_eq!(meeting.render_device_id.as_deref(), Some("speaker"));
    assert_eq!(meeting.display.as_deref(), Some("DISPLAY2"));
    assert_eq!(meeting.window, Some(77));
    assert_eq!(meeting.process_id, Some(100));
}

#[test]
fn does_not_start_on_microphone_use_shorter_than_the_grace_period() {
    let (mut d, t) = detector();
    assert_eq!(d.update(t.at(0.0), &start("Zoom")), None);
    assert_eq!(d.update(t.at(2.0), NOTHING), None);
    assert_eq!(d.update(t.at(4.0), &start("Zoom")), None);
    assert_eq!(d.update(t.at(6.9), &start("Zoom")), None);
    assert_eq!(started_rule(d.update(t.at(7.0), &start("Zoom"))), "Zoom");
}

#[test]
fn start_is_notified_only_once() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    started_rule(d.update(t.at(3.0), &start("Zoom")));
    assert_eq!(d.update(t.at(4.0), &start("Zoom")), None);
    assert_eq!(d.update(t.at(100.0), &start("Zoom")), None);
}

#[test]
fn after_start_continues_while_the_microphone_is_in_use_even_if_the_title_no_longer_matches() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("WebMeeting"));
    d.update(t.at(3.0), &start("WebMeeting"));
    assert_eq!(d.update(t.at(60.0), &mic_only("WebMeeting")), None);
    assert_eq!(d.update(t.at(3600.0), &mic_only("WebMeeting")), None);
    assert!(d.current().is_some());
}

#[test]
fn ends_when_microphone_use_lapses_for_the_grace_period() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    assert_eq!(d.update(t.at(100.0), NOTHING), None);
    assert_eq!(d.update(t.at(114.9), NOTHING), None);
    assert_eq!(ended_rule(d.update(t.at(115.0), NOTHING)), "Zoom");
    assert!(d.current().is_none());
}

#[test]
fn does_not_end_on_a_lapse_shorter_than_the_grace_period() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    assert_eq!(d.update(t.at(100.0), NOTHING), None);
    assert_eq!(d.update(t.at(110.0), &mic_only("Zoom")), None);
    assert_eq!(d.update(t.at(120.0), NOTHING), None);
    assert_eq!(d.update(t.at(134.9), NOTHING), None);
    assert_eq!(ended_rule(d.update(t.at(135.0), NOTHING)), "Zoom");
}

#[test]
fn microphone_use_by_another_rule_does_not_keep_the_meeting_going() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    assert_eq!(d.update(t.at(100.0), &mic_only("WebMeeting")), None);
    assert_eq!(ended_rule(d.update(t.at(115.0), &mic_only("WebMeeting"))), "Zoom");
}

#[test]
fn the_next_meeting_can_start_after_the_end() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    d.update(t.at(100.0), NOTHING);
    ended_rule(d.update(t.at(115.0), NOTHING));
    assert_eq!(d.update(t.at(200.0), &start("Teams")), None);
    assert_eq!(started_rule(d.update(t.at(203.0), &start("Teams"))), "Teams");
}

#[test]
fn with_a_zero_grace_period_start_and_end_are_notified_immediately() {
    let mut d = MeetingDetector::new(Duration::ZERO, Duration::ZERO);
    let t = Clock(Instant::now());
    assert_eq!(started_rule(d.update(t.at(0.0), &start("Zoom"))), "Zoom");
    assert_eq!(ended_rule(d.update(t.at(1.0), NOTHING)), "Zoom");
}
