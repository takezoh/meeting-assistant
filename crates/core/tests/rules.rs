use meeting_assistant_core::fakes::{MIC, process, snapshot, window, window_id, zoom_in_meeting};
use meeting_assistant_core::{
    AudioSession, BrowserLink, BrowserMeeting, BrowserTab, ConfigError, MeetingRule, RuleEvaluator, RuleEvidence,
    SignalSnapshot, default_rules,
};

fn evaluate(snapshot: &SignalSnapshot) -> Vec<RuleEvidence> {
    RuleEvaluator::new(&default_rules()).unwrap().evaluate(snapshot)
}

fn evidence(snapshot: &SignalSnapshot, rule: &str) -> RuleEvidence {
    evaluate(snapshot).into_iter().find(|e| e.rule_name == rule).unwrap()
}

/// Chrome (200) using the microphone through its audio process (201), with a meeting title.
fn chrome_using_the_microphone() -> SignalSnapshot {
    snapshot(
        &[process(200, 1, "chrome.exe"), process(201, 200, "chrome.exe")],
        &[201],
        &[],
        &[window(200, "Meet - abc-defg-hij - Google Chrome", "DISPLAY1", 1000)],
    )
}

#[test]
fn a_desktop_app_using_the_microphone_is_active() {
    let e = evidence(&zoom_in_meeting(), "Zoom");
    assert!(e.active);
    assert_eq!(e.capture_device_id.as_deref(), Some(MIC));
}

#[test]
fn merely_running_without_using_the_microphone_is_not_active() {
    let s = snapshot(&[process(100, 1, "Zoom.exe")], &[], &[], &[]);
    assert!(evaluate(&s).iter().all(|e| !e.active));
}

#[test]
fn a_child_process_of_the_target_app_using_the_microphone_counts_as_the_target_app() {
    let s = snapshot(
        &[
            process(100, 1, "ms-teams.exe"),
            process(101, 100, "msedgewebview2.exe"),
            process(102, 101, "msedgewebview2.exe"),
        ],
        &[102],
        &[],
        &[],
    );
    assert!(evidence(&s, "Teams").active);
}

#[test]
fn an_unrelated_process_using_the_microphone_is_not_a_meeting() {
    let s = snapshot(&[process(100, 1, "VoiceRecorder.exe")], &[100], &[], &[]);
    assert!(evaluate(&s).iter().all(|e| !e.active));
}

#[test]
fn a_browser_is_not_detected_without_the_extension_even_with_a_meeting_title() {
    let s = chrome_using_the_microphone();
    assert!(evaluate(&s).iter().all(|e| !e.active), "{:?}", evaluate(&s));
}

#[test]
fn a_desktop_app_launched_from_a_browser_belongs_to_the_app() {
    let s = snapshot(
        &[process(200, 1, "chrome.exe"), process(100, 200, "Zoom.exe")],
        &[100],
        &[],
        &[],
    );
    let e = evidence(&s, "Zoom");
    assert!(e.active);
    assert_eq!(e.process_id, Some(100), "the browser is not the audio target");
}

#[test]
fn every_window_of_the_app_and_its_display_are_evidence_and_no_other_apps() {
    let s = snapshot(
        &[process(100, 1, "Zoom.exe"), process(200, 1, "Notes.exe")],
        &[100],
        &[],
        &[
            window(100, "Zoom", "DISPLAY1", 100),
            window(100, "Zoom Meeting", "DISPLAY2", 9000),
            window(200, "Notes", "DISPLAY1", 5000),
        ],
    );
    let e = evidence(&s, "Zoom");
    let windows: Vec<(u64, Option<&str>)> = e.windows.iter().map(|w| (w.id, w.display.as_deref())).collect();
    assert_eq!(
        windows,
        [
            (window_id("Zoom"), Some("DISPLAY1")),
            (window_id("Zoom Meeting"), Some("DISPLAY2"))
        ]
    );
}

#[test]
fn the_playback_device_is_the_one_the_same_app_is_playing_sound_to() {
    let mut s = snapshot(
        &[process(100, 1, "Zoom.exe"), process(300, 1, "spotify.exe")],
        &[100],
        &[],
        &[],
    );
    s.render_sessions = vec![
        AudioSession {
            process_id: 300,
            device_id: "speaker-music".into(),
        },
        AudioSession {
            process_id: 100,
            device_id: "headset".into(),
        },
    ];
    assert_eq!(evidence(&s, "Zoom").render_device_id.as_deref(), Some("headset"));
}

#[test]
fn the_default_rules_know_the_macos_executables_of_the_apps() {
    for (name, rule) in [("zoom.us", "Zoom"), ("MSTeams", "Teams"), ("Slack", "Slack")] {
        let s = snapshot(&[process(100, 1, name)], &[100], &[], &[]);
        assert!(evidence(&s, rule).active, "{name}");
    }
}

#[test]
fn process_names_ignore_differences_in_extension_and_letter_case() {
    for name in ["ZOOM.EXE", "zoom", "Zoom.exe"] {
        let s = snapshot(&[process(100, 1, name)], &[100], &[], &[]);
        assert!(evidence(&s, "Zoom").active, "{name}");
    }
}

#[test]
fn evaluation_finishes_even_if_the_parent_child_relationship_is_cyclic() {
    let s = SignalSnapshot {
        capture_sessions: vec![AudioSession {
            process_id: 100,
            device_id: MIC.into(),
        }],
        processes: vec![process(100, 101, "a.exe"), process(101, 100, "b.exe")],
        ..Default::default()
    };
    assert!(evaluate(&s).iter().all(|e| !e.active));
}

#[test]
fn a_rule_without_process_names_is_a_config_error() {
    let rule = MeetingRule {
        name: "Bad".into(),
        process_names: Vec::new(),
    };
    let error = RuleEvaluator::new(&[rule]).unwrap_err();
    assert!(
        matches!(&error, ConfigError::InvalidRule { rule, .. } if rule == "Bad"),
        "{error}"
    );
}

#[test]
fn the_audio_target_is_the_top_level_process_found_by_walking_up_same_named_parents() {
    let s = snapshot(
        &[
            process(100, 1, "Zoom.exe"),
            process(101, 100, "Zoom.exe"),
            process(102, 101, "Zoom.exe"),
        ],
        &[102],
        &[],
        &[],
    );
    assert_eq!(evidence(&s, "Zoom").process_id, Some(100));
}

#[test]
fn a_child_process_with_a_different_name_using_the_microphone_still_makes_the_app_process_the_audio_target() {
    let s = snapshot(
        &[
            process(100, 1, "ms-teams.exe"),
            process(101, 100, "msedgewebview2.exe"),
            process(102, 101, "msedgewebview2.exe"),
        ],
        &[102],
        &[],
        &[],
    );
    assert_eq!(evidence(&s, "Teams").process_id, Some(100));
}

#[test]
fn there_is_no_audio_target_when_the_microphone_is_not_in_use() {
    let s = snapshot(
        &[process(100, 1, "Zoom.exe")],
        &[],
        &[],
        &[window(100, "Zoom", "DISPLAY1", 100)],
    );
    assert_eq!(evidence(&s, "Zoom").process_id, None);
}

/// Chrome (200) using the microphone, whose relay process (301) to the extension it launched via cmd (300).
fn chrome_with_extension_host() -> SignalSnapshot {
    let mut s = chrome_using_the_microphone();
    s.processes.push(process(300, 200, "cmd.exe"));
    s.processes.push(process(301, 300, "meeting-assistant.exe"));
    s
}

fn linked(mut snapshot: SignalSnapshot, meeting: bool) -> SignalSnapshot {
    snapshot.browser_links = vec![BrowserLink {
        id: 1,
        host_process_id: 301,
    }];
    if meeting {
        snapshot.browser_meetings = vec![BrowserMeeting {
            tab: BrowserTab { link: 1, tab: 42 },
            service: "GoogleMeet".into(),
            title: None,
        }];
    }
    snapshot
}

#[test]
fn a_meeting_reported_by_the_extension_is_active_by_itself() {
    let s = linked(snapshot(&[], &[], &[], &[]), true);
    let e = evidence(&s, "GoogleMeet");
    assert!(e.active);
    assert_eq!(e.browser_tab, Some(BrowserTab { link: 1, tab: 42 }));
    // It sorts before the rules (because whichever is found first is adopted).
    assert_eq!(evaluate(&s)[0].rule_name, "GoogleMeet");
    assert_eq!(e.title, None);
}

#[test]
fn the_title_the_extension_reports_stays_with_its_meeting() {
    let mut s = linked(snapshot(&[], &[], &[], &[]), true);
    s.browser_meetings[0].title = Some("abc-defg-hij".into());

    assert_eq!(evidence(&s, "GoogleMeet").title.as_deref(), Some("abc-defg-hij"));
    // Rules detected from the microphone have no title.
    assert_eq!(evidence(&s, "Zoom").title, None);
}

#[test]
fn a_browser_connected_to_the_extension_is_left_to_it_even_under_a_users_own_rule() {
    let rules = [MeetingRule {
        name: "MyBrowserRule".into(),
        process_names: vec!["chrome".into()],
    }];
    let evaluator = RuleEvaluator::new(&rules).unwrap();
    let rule_evidence = |s: &SignalSnapshot| {
        evaluator
            .evaluate(s)
            .into_iter()
            .find(|e| e.rule_name == "MyBrowserRule")
    };

    assert!(rule_evidence(&chrome_with_extension_host()).unwrap().active);
    assert!(
        !rule_evidence(&linked(chrome_with_extension_host(), false))
            .unwrap()
            .active,
        "one call is not detected twice"
    );
}

#[test]
fn a_desktop_app_is_detected_as_before_even_when_the_extension_is_connected() {
    let mut s = chrome_with_extension_host();
    s.processes.push(process(100, 1, "Zoom.exe"));
    s.capture_sessions = vec![AudioSession {
        process_id: 100,
        device_id: MIC.into(),
    }];

    assert!(evidence(&linked(s, false), "Zoom").active);
}
