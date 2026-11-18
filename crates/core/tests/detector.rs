use std::time::{Duration, Instant};

use meeting_assistant_core::{BrowserTab, DetectorEvent, MeetingDetector, RuleEvidence, Timing};

struct Clock(Instant);

impl Clock {
    fn at(&self, seconds: f64) -> Instant {
        self.0 + Duration::from_secs_f64(seconds)
    }
}

fn timing(start_debounce: f64, end_grace: f64) -> Timing {
    Timing {
        start_debounce: Duration::from_secs_f64(start_debounce),
        end_grace: Duration::from_secs_f64(end_grace),
    }
}

/// Desktop apps: start after 3 s, end after 15 s. Browser meetings: start after 1 s, end after 5 s.
fn detector() -> (MeetingDetector, Clock) {
    (
        MeetingDetector::new(timing(3.0, 15.0), timing(1.0, 5.0)),
        Clock(Instant::now()),
    )
}

fn active(rule: &str) -> RuleEvidence {
    RuleEvidence {
        rule_name: rule.into(),
        active: true,
        capture_device_id: Some("mic".into()),
        render_device_id: Some("speaker".into()),
        process_id: Some(100),
        window: Some(77),
        display: Some("DISPLAY2".into()),
        browser_tab: None,
        title: None,
    }
}

fn start(rule: &str) -> Vec<RuleEvidence> {
    vec![active(rule)]
}

/// A meeting reported by the extension in tab `tab`.
fn tab(service: &str, tab: u64) -> RuleEvidence {
    RuleEvidence {
        rule_name: service.into(),
        active: true,
        capture_device_id: None,
        render_device_id: None,
        process_id: None,
        window: None,
        display: None,
        browser_tab: Some(BrowserTab { link: 1, tab }),
        title: None,
    }
}

const NOTHING: &[RuleEvidence] = &[];

fn started_rule(events: Vec<DetectorEvent>) -> String {
    match <[DetectorEvent; 1]>::try_from(events) {
        Ok([DetectorEvent::Started(meeting)]) => meeting.rule_name,
        other => panic!("not one start event: {other:?}"),
    }
}

fn ended_rule(events: Vec<DetectorEvent>) -> String {
    match <[DetectorEvent; 1]>::try_from(events) {
        Ok([DetectorEvent::Ended(meeting)]) => meeting.rule_name,
        other => panic!("not one end event: {other:?}"),
    }
}

fn detected(d: &MeetingDetector) -> Vec<String> {
    d.meetings().map(|m| m.rule_name.clone()).collect()
}

#[test]
fn does_not_start_until_the_start_condition_lasts_for_the_grace_period() {
    let (mut d, t) = detector();
    assert!(d.update(t.at(0.0), &start("Zoom")).is_empty());
    assert!(d.update(t.at(2.9), &start("Zoom")).is_empty());
    assert_eq!(started_rule(d.update(t.at(3.0), &start("Zoom"))), "Zoom");
}

#[test]
fn a_started_meeting_has_the_observed_device_window_and_process() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    let Ok([DetectorEvent::Started(meeting)]) = <[DetectorEvent; 1]>::try_from(d.update(t.at(3.0), &start("Zoom")))
    else {
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
    assert!(d.update(t.at(0.0), &start("Zoom")).is_empty());
    assert!(d.update(t.at(2.0), NOTHING).is_empty());
    assert!(d.update(t.at(4.0), &start("Zoom")).is_empty());
    assert!(d.update(t.at(6.9), &start("Zoom")).is_empty());
    assert_eq!(started_rule(d.update(t.at(7.0), &start("Zoom"))), "Zoom");
}

#[test]
fn start_is_notified_only_once() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    started_rule(d.update(t.at(3.0), &start("Zoom")));
    assert!(d.update(t.at(4.0), &start("Zoom")).is_empty());
    assert!(d.update(t.at(100.0), &start("Zoom")).is_empty());
}

#[test]
fn ends_when_microphone_use_lapses_for_the_grace_period() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    assert!(d.update(t.at(100.0), NOTHING).is_empty());
    assert!(d.update(t.at(114.9), NOTHING).is_empty());
    assert_eq!(ended_rule(d.update(t.at(115.0), NOTHING)), "Zoom");
    assert!(detected(&d).is_empty());
}

#[test]
fn does_not_end_on_a_lapse_shorter_than_the_grace_period() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    assert!(d.update(t.at(100.0), NOTHING).is_empty());
    assert!(d.update(t.at(110.0), &start("Zoom")).is_empty());
    assert!(d.update(t.at(120.0), NOTHING).is_empty());
    assert!(d.update(t.at(134.9), NOTHING).is_empty());
    assert_eq!(ended_rule(d.update(t.at(135.0), NOTHING)), "Zoom");
}

#[test]
fn another_meeting_being_active_does_not_keep_this_one_going() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    d.update(t.at(100.0), &start("Teams"));
    let events = d.update(t.at(115.0), &start("Teams"));
    assert!(
        matches!(&events[0], DetectorEvent::Ended(m) if m.rule_name == "Zoom"),
        "{events:?}"
    );
}

#[test]
fn the_next_meeting_can_start_after_the_end() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    d.update(t.at(3.0), &start("Zoom"));
    d.update(t.at(100.0), NOTHING);
    ended_rule(d.update(t.at(115.0), NOTHING));
    assert!(d.update(t.at(200.0), &start("Teams")).is_empty());
    assert_eq!(started_rule(d.update(t.at(203.0), &start("Teams"))), "Teams");
}

#[test]
fn with_a_zero_grace_period_start_and_end_are_notified_immediately() {
    let mut d = MeetingDetector::new(timing(0.0, 0.0), timing(0.0, 0.0));
    let t = Clock(Instant::now());
    assert_eq!(started_rule(d.update(t.at(0.0), &start("Zoom"))), "Zoom");
    assert_eq!(ended_rule(d.update(t.at(1.0), NOTHING)), "Zoom");
}

#[test]
fn a_meeting_that_starts_during_another_is_detected_too_and_each_ends_on_its_own() {
    let (mut d, t) = detector();
    d.update(t.at(0.0), &start("Zoom"));
    started_rule(d.update(t.at(3.0), &start("Zoom")));

    let both = [active("Zoom"), active("Teams")];
    assert!(d.update(t.at(10.0), &both).is_empty());
    assert_eq!(started_rule(d.update(t.at(13.0), &both)), "Teams");
    assert_eq!(detected(&d), ["Zoom", "Teams"], "in the order they started");

    // Zoom stops using the microphone; Teams goes on.
    assert!(d.update(t.at(20.0), &start("Teams")).is_empty());
    assert_eq!(ended_rule(d.update(t.at(35.0), &start("Teams"))), "Zoom");
    assert_eq!(detected(&d), ["Teams"]);
}

#[test]
fn two_tabs_of_the_same_service_are_two_meetings() {
    let mut d = MeetingDetector::new(timing(0.0, 0.0), timing(0.0, 0.0));
    let t = Clock(Instant::now());
    let events = d.update(t.at(0.0), &[tab("GoogleMeet", 1), tab("GoogleMeet", 2)]);
    assert_eq!(events.len(), 2);

    // The first tab hangs up; the call in the second tab does not keep it alive.
    let Ok([DetectorEvent::Ended(ended)]) =
        <[DetectorEvent; 1]>::try_from(d.update(t.at(1.0), &[tab("GoogleMeet", 2)]))
    else {
        panic!()
    };
    assert_eq!(ended.browser_tab, Some(BrowserTab { link: 1, tab: 1 }));
    assert_eq!(d.meetings().count(), 1);
}

#[test]
fn ends_are_reported_before_starts_in_the_same_update() {
    let mut d = MeetingDetector::new(timing(0.0, 0.0), timing(0.0, 0.0));
    let t = Clock(Instant::now());
    d.update(t.at(0.0), &start("Zoom"));

    let events = d.update(t.at(1.0), &start("Teams"));

    assert!(
        matches!(&events[..], [DetectorEvent::Ended(z), DetectorEvent::Started(m)]
        if z.rule_name == "Zoom" && m.rule_name == "Teams")
    );
}

#[test]
fn a_browser_meeting_starts_and_ends_on_its_own_shorter_timing() {
    let (mut d, t) = detector();
    let meet = [tab("GoogleMeet", 1)];
    assert!(d.update(t.at(0.0), &meet).is_empty());
    assert_eq!(started_rule(d.update(t.at(1.0), &meet)), "GoogleMeet");

    assert!(d.update(t.at(10.0), NOTHING).is_empty());
    assert!(d.update(t.at(14.9), NOTHING).is_empty());
    assert_eq!(ended_rule(d.update(t.at(15.0), NOTHING)), "GoogleMeet");
}

#[test]
fn a_desktop_app_keeps_its_longer_timing_while_a_browser_meeting_uses_its_own() {
    let (mut d, t) = detector();
    let both = [active("Zoom"), tab("GoogleMeet", 1)];
    assert!(d.update(t.at(0.0), &both).is_empty());
    assert_eq!(started_rule(d.update(t.at(1.0), &both)), "GoogleMeet");
    assert!(d.update(t.at(2.9), &both).is_empty());
    assert_eq!(started_rule(d.update(t.at(3.0), &both)), "Zoom");

    // Both stop at 10 s: the browser meeting ends 5 s later, the app 15 s later.
    assert!(d.update(t.at(10.0), NOTHING).is_empty());
    assert_eq!(ended_rule(d.update(t.at(15.0), NOTHING)), "GoogleMeet");
    assert!(d.update(t.at(24.9), NOTHING).is_empty());
    assert_eq!(ended_rule(d.update(t.at(25.0), NOTHING)), "Zoom");
}
