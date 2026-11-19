use std::time::{Duration, Instant};

use meeting_assistant_core::fakes::{
    FakeRecorder, ScriptedSignals, fake_ports, process, snapshot, window, window_id, zoom_in_meeting,
};
use meeting_assistant_core::{
    BrowserLink, BrowserMeeting, BrowserTab, Config, Engine, NoticeLevel, RecordingCoordinator, VideoTarget,
};

struct Fixture {
    recorder: FakeRecorder,
    signals: ScriptedSignals,
    engine: Engine<ScriptedSignals, FakeRecorder>,
    now: Instant,
}

/// Built with the default settings (start grace 3 s, end grace 2 s, signal outage limit 60 s).
fn fixture() -> Fixture {
    let recorder = FakeRecorder::default();
    let signals = ScriptedSignals::default();
    let coordinator = RecordingCoordinator::new(recorder.clone(), fake_ports(&recorder));
    let engine = Engine::new(signals.clone(), coordinator, &Config::default()).unwrap();
    Fixture {
        recorder,
        signals,
        engine,
        now: Instant::now(),
    }
}

impl Fixture {
    /// Advances detection with the same signals, `seconds` times at 1-second intervals.
    fn run(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.engine.tick(self.now);
            self.now += Duration::from_secs(1);
        }
    }

    fn recording(&self) -> bool {
        self.recorder.state().recording
    }
}

#[test]
fn records_a_desktop_app_meeting_from_start_to_end() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());
    f.run(10);
    assert!(f.recording());
    assert!(
        f.recorder.state().started[0]
            .output_path
            .ends_with("2026-11-17_10-00-00_Zoom.mp4")
    );

    f.signals.set(snapshot(&[process(100, 1, "Zoom.exe")], &[], &[], &[]));
    f.run(20);
    assert!(!f.recording());
    assert_eq!(f.recorder.state().stop_count, 1);
}

#[test]
fn does_not_record_while_no_meeting_is_taking_place() {
    let mut f = fixture();
    // Zoom is open but silent; a voice recorder and a browser (without the extension) use the microphone.
    f.signals.set(snapshot(
        &[
            process(100, 1, "Zoom.exe"),
            process(200, 1, "chrome.exe"),
            process(300, 1, "VoiceRecorder.exe"),
        ],
        &[300, 200],
        &[],
        &[window(200, "Meet - abc-defg-hij - Google Chrome", "DISPLAY1", 1000)],
    ));
    f.run(120);

    assert!(f.recorder.state().started.is_empty());
}

#[test]
fn does_not_stop_recording_when_signal_acquisition_fails_transiently() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());
    f.run(10);

    f.signals.set_failure(Some("audio service restarting"));
    f.run(30);
    f.signals.set_failure(None);
    f.run(30);

    assert!(f.recording());
    assert_eq!(f.recorder.state().stop_count, 0);
    assert!(f.engine.take_notices().iter().all(|n| n.level == NoticeLevel::Info));
}

#[test]
fn stops_recording_and_notifies_when_signal_acquisition_keeps_failing() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());
    f.run(10);

    f.signals.set_failure(Some("audio service gone"));
    f.run(120);

    assert!(!f.recording());
    let errors = f
        .engine
        .take_notices()
        .into_iter()
        .filter(|n| n.level == NoticeLevel::Error)
        .count();
    assert_eq!(errors, 1);
}

#[test]
fn resumes_on_the_next_cycle_when_recording_fails_during_a_meeting() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());
    f.run(10);

    f.recorder.fail("device lost");
    f.run(2);

    assert!(f.recording());
    assert_eq!(f.recorder.state().started.len(), 2);
}

fn zoom_with(windows: &[(&str, u64)]) -> meeting_assistant_core::SignalSnapshot {
    let windows: Vec<_> = windows
        .iter()
        .map(|(title, area)| window(100, title, "DISPLAY1", *area))
        .collect();
    snapshot(&[process(100, 1, "Zoom.exe")], &[100], &[], &windows)
}

#[test]
fn records_no_window_of_the_app_that_was_open_before_its_microphone_was_taken() {
    let mut f = fixture();
    // The windows are read first when the microphone is taken; those open then are the app's own.
    f.signals.set(zoom_with(&[("Zoom", 9000), ("Waiting room", 5000)]));
    f.run(10);
    assert_eq!(
        f.recorder.state().started[0].video,
        VideoTarget::Blank,
        "no picture rather than the app's main window"
    );
    assert!(f.recorder.state().retargets.is_empty());
}

#[test]
fn moves_to_a_window_that_appears_after_the_recorded_one_closes() {
    let mut f = fixture();
    f.signals.set(zoom_with(&[("Zoom", 9000)]));
    f.run(1);
    f.signals.set(zoom_with(&[("Zoom", 9000), ("Zoom Meeting", 5000)]));
    f.run(5);
    assert_eq!(
        f.recorder.state().started[0].video,
        VideoTarget::Window(window_id("Zoom Meeting"))
    );

    // The meeting window closes; the app's main window is not taken in its place.
    f.signals.set(zoom_with(&[("Zoom", 9000)]));
    f.run(5);
    assert!(f.recording());
    assert!(f.recorder.state().retargets.is_empty());

    // Another window appears during the meeting: it is recorded.
    f.signals.set(zoom_with(&[("Zoom", 9000), ("Zoom Meeting 2", 5000)]));
    f.run(2);
    assert_eq!(f.recorder.state().retargets, [window_id("Zoom Meeting 2")]);
    assert_eq!(f.recorder.state().started.len(), 1);
}

#[test]
fn does_not_move_when_another_window_appears_while_the_recorded_window_remains() {
    let mut f = fixture();
    f.signals.set(zoom_with(&[]));
    f.run(1);
    f.signals.set(zoom_with(&[("Zoom Meeting", 5000)]));
    f.run(10);

    f.signals
        .set(zoom_with(&[("Zoom Meeting", 5000), ("Shared screen", 9000)]));
    f.run(30);

    assert!(f.recorder.state().retargets.is_empty());
}

#[test]
fn records_the_call_window_until_it_closes_and_stops_soon_after_without_moving_to_the_main_window() {
    let mut f = fixture();
    let main = window(100, "Zoom", "DISPLAY1", 9000);
    let call = window(100, "Zoom Meeting", "DISPLAY1", 5000);
    let zoom = |microphone: bool, windows: &[meeting_assistant_core::WindowEntry]| {
        let users: &[u32] = if microphone { &[100] } else { &[] };
        snapshot(&[process(100, 1, "Zoom.exe")], users, &[], windows)
    };
    // Joining takes the microphone, and the call window opens.
    f.signals.set(zoom(true, std::slice::from_ref(&main)));
    f.run(1);
    f.signals.set(zoom(true, &[main.clone(), call.clone()]));
    f.run(5);
    assert_eq!(
        f.recorder.state().started[0].video,
        VideoTarget::Window(window_id("Zoom Meeting")),
        "the window that appeared with the meeting, not the larger main window"
    );

    // Muted in an app that lets go of the microphone: the call window keeps the recording going.
    f.signals.set(zoom(false, &[main.clone(), call.clone()]));
    f.run(30);
    assert!(f.recording());

    // Leaving closes the call window.
    f.signals.set(zoom(false, std::slice::from_ref(&main)));
    f.run(3);
    assert!(!f.recording(), "ends after the 2-second grace");
    assert!(
        f.recorder.state().retargets.is_empty(),
        "the grace is not spent recording the main window"
    );
}

#[test]
fn a_recording_started_without_a_window_moves_to_a_window_that_appears() {
    let mut f = fixture();
    f.signals.set(zoom_with(&[]));
    f.run(10);
    assert_eq!(f.recorder.state().started[0].video, VideoTarget::Blank);

    f.signals.set(zoom_with(&[("Zoom Meeting", 5000)]));
    f.run(2);

    assert_eq!(f.recorder.state().retargets, [window_id("Zoom Meeting")]);
}

#[test]
fn lists_every_meeting_in_progress_and_records_one_at_a_time() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());
    f.run(10);

    let tab = BrowserTab { link: 1, tab: 42 };
    let mut both = zoom_in_meeting();
    both.browser_links = vec![BrowserLink {
        id: 1,
        host_process_id: 301,
    }];
    both.browser_meetings = vec![BrowserMeeting {
        tab,
        service: "GoogleMeet".into(),
        title: None,
    }];
    f.signals.set(both.clone());
    f.run(10);

    let status = f.engine.status();
    let names: Vec<_> = status.meetings.iter().map(|m| m.key.rule_name.as_str()).collect();
    assert_eq!(names, ["Zoom", "GoogleMeet"]);
    assert!(status.is_recording_meeting(&status.meetings[0].key));
    assert_eq!(f.recorder.state().started.len(), 1);

    // Zoom hangs up; the call in the tab is recorded next.
    both.capture_sessions.clear();
    f.signals.set(both);
    f.run(20);
    let state = f.recorder.state();
    assert_eq!(state.stop_count, 1);
    assert_eq!(state.started.len(), 2);
    assert_eq!(state.started[1].video, VideoTarget::BrowserTab(tab));
}

#[test]
fn records_a_meeting_reported_by_the_extension_in_its_tab_from_start_to_end() {
    let mut f = fixture();
    let tab = BrowserTab { link: 1, tab: 42 };
    let mut in_meeting = snapshot(&[], &[], &[], &[]);
    in_meeting.browser_links = vec![BrowserLink {
        id: 1,
        host_process_id: 301,
    }];
    in_meeting.browser_meetings = vec![BrowserMeeting {
        tab,
        service: "GoogleMeet".into(),
        title: None,
    }];
    f.signals.set(in_meeting.clone());
    f.run(10);
    {
        let state = f.recorder.state();
        assert_eq!(state.started.len(), 1);
        assert_eq!(state.started[0].video, VideoTarget::BrowserTab(tab));
    }

    in_meeting.browser_meetings.clear();
    f.signals.set(in_meeting);
    f.run(20);
    assert!(!f.recording());
    assert_eq!(f.recorder.state().stop_count, 1);
}

#[test]
fn reads_only_the_microphone_sessions_while_no_app_uses_the_microphone() {
    let mut f = fixture();
    f.signals.set(snapshot(
        &[process(100, 1, "Zoom.exe")],
        &[],
        &[100],
        &[window(100, "Zoom", "DISPLAY1", 1000)],
    ));

    f.run(3);

    assert_eq!(f.signals.take_reads(), ["capture_sessions"; 3]);
}

#[test]
fn reads_the_processes_windows_and_playback_once_an_app_uses_the_microphone() {
    let mut f = fixture();
    f.signals.set(zoom_in_meeting());

    f.run(1);

    assert_eq!(
        f.signals.take_reads(),
        ["capture_sessions", "render_sessions", "processes", "windows"]
    );
}

#[test]
fn keeps_reading_the_windows_while_an_apps_meeting_is_recorded_without_the_microphone() {
    let mut f = fixture();
    f.signals.set(snapshot(
        &[process(100, 1, "Zoom.exe")],
        &[100],
        &[],
        &[window(100, "Zoom Meeting", "DISPLAY1", 1000)],
    ));
    f.run(10);
    assert!(f.recording());

    // The app lets the microphone go for a moment, within the end grace, and its window closes meanwhile.
    let other_window = window(100, "Zoom Workplace", "DISPLAY1", 800);
    f.signals
        .set(snapshot(&[process(100, 1, "Zoom.exe")], &[], &[], &[other_window]));
    f.signals.take_reads();
    f.run(1);

    assert_eq!(f.signals.take_reads(), ["capture_sessions", "processes", "windows"]);
    assert_eq!(
        f.recorder.state().retargets,
        [window_id("Zoom Workplace")],
        "the recording follows the app's other window"
    );
}
