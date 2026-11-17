use meeting_assistant_core::fakes::{FakeOutputFolder, FakeRecorder, RecordedPauses, fake_ports_with, output_path};
use meeting_assistant_core::{
    BrowserTab, CaptureScope, DetectorEvent, MeetingInfo, NoticeLevel, RecordingCoordinator, RecordingTrigger,
    VideoTarget,
};

type Coordinator = RecordingCoordinator<FakeRecorder>;

/// A coordinator built only from fakes, and those fakes.
struct Fixture {
    recorder: FakeRecorder,
    folder: FakeOutputFolder,
    pauses: RecordedPauses,
    coordinator: Coordinator,
}

fn fixture() -> Fixture {
    let recorder = FakeRecorder::default();
    let folder = FakeOutputFolder::new(&recorder);
    let pauses = RecordedPauses::default();
    let coordinator = Coordinator::new(recorder.clone(), fake_ports_with(folder.clone(), pauses.clone()));
    Fixture {
        recorder,
        folder,
        pauses,
        coordinator,
    }
}

/// A coordinator with the recording scope set to the whole display. Shares the fakes of `f`.
fn screen_scope(f: &Fixture) -> Coordinator {
    Coordinator::new(f.recorder.clone(), fake_ports_with(f.folder.clone(), f.pauses.clone()))
        .with_capture_scope(CaptureScope::Screen)
}

fn zoom() -> MeetingInfo {
    MeetingInfo {
        rule_name: "Zoom".into(),
        capture_device_id: Some("mic".into()),
        render_device_id: Some("speaker".into()),
        process_id: Some(100),
        window: Some(77),
        display: Some("DISPLAY2".into()),
        browser_tab: None,
    }
}

fn started(c: &mut Coordinator) {
    c.handle(DetectorEvent::Started(zoom()));
}

fn ended(c: &mut Coordinator) {
    c.handle(DetectorEvent::Ended(zoom()));
}

fn has_notice(c: &mut Coordinator, level: NoticeLevel) -> bool {
    c.take_notices().iter().any(|n| n.level == level)
}

#[test]
fn meeting_start_begins_recording_the_apps_window_and_sound() {
    let mut f = fixture();
    started(&mut f.coordinator);

    let state = f.recorder.state();
    assert_eq!(state.started.len(), 1);
    let request = &state.started[0];
    assert_eq!(request.output_path, output_path("2026-11-17_10-00-00_Zoom.mp4"));
    assert_eq!(request.video, VideoTarget::Window(77));
    assert_eq!(request.audio_process_id, Some(100));
    assert_eq!(request.capture_device_id.as_deref(), Some("mic"));
    assert_eq!(request.render_device_id.as_deref(), Some("speaker"));
    let status = f.coordinator.status();
    assert!(status.is_recording());
    assert_eq!(status.recording_trigger, Some(RecordingTrigger::Auto));
}

#[test]
fn meeting_end_stops_the_recording() {
    let mut f = fixture();
    started(&mut f.coordinator);
    ended(&mut f.coordinator);

    assert_eq!(f.recorder.state().stop_count, 1);
    assert!(!f.coordinator.status().is_recording());
}

#[test]
fn prepares_the_output_folder_before_starting_a_recording() {
    let mut f = fixture();
    started(&mut f.coordinator);

    assert_eq!(f.folder.state().prepared, 1);
    assert_eq!(f.recorder.state().started.len(), 1);
}

#[test]
fn notifies_an_error_without_recording_when_the_output_folder_cannot_be_prepared() {
    let mut f = fixture();
    f.folder.state().prepare_failure = Some("read-only".into());
    started(&mut f.coordinator);

    assert!(f.recorder.state().started.is_empty());
    assert!(!f.coordinator.status().is_recording());
    assert!(has_notice(&mut f.coordinator, NoticeLevel::Error));
}

#[test]
fn adds_a_sequence_number_when_a_file_with_the_same_name_already_exists() {
    let mut f = fixture();
    f.folder.state().files = vec![
        "2026-11-17_10-00-00_Zoom.mp4".into(),
        "2026-11-17_10-00-00_Zoom_2.mp4".into(),
    ];
    started(&mut f.coordinator);

    assert_eq!(
        f.recorder.state().started[0].output_path,
        output_path("2026-11-17_10-00-00_Zoom_3.mp4")
    );
}

#[test]
fn does_not_record_when_a_meeting_starts_with_auto_recording_disabled() {
    let mut f = fixture();
    f.coordinator.set_auto_enabled(false);
    started(&mut f.coordinator);

    assert!(f.recorder.state().started.is_empty());
    assert_eq!(f.coordinator.status().meeting.as_deref(), Some("Zoom"));
}

#[test]
fn re_enabling_auto_recording_during_a_meeting_starts_recording() {
    let mut f = fixture();
    f.coordinator.set_auto_enabled(false);
    started(&mut f.coordinator);
    f.coordinator.set_auto_enabled(true);

    assert_eq!(f.recorder.state().started.len(), 1);
}

#[test]
fn disabling_auto_recording_does_not_stop_a_recording_in_progress() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.set_auto_enabled(false);

    assert_eq!(f.recorder.state().stop_count, 0);
    assert!(f.coordinator.status().is_recording());
}

#[test]
fn after_auto_recording_is_stopped_manually_it_does_not_resume_for_that_meeting() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.stop_manual();
    f.coordinator.poll_recorder();
    ended(&mut f.coordinator);

    let state = f.recorder.state();
    assert_eq!(state.started.len(), 1);
    assert_eq!(state.stop_count, 1);
}

#[test]
fn a_manually_started_recording_continues_after_the_meeting_ends() {
    let mut f = fixture();
    f.coordinator.start_manual();
    started(&mut f.coordinator);
    ended(&mut f.coordinator);

    assert_eq!(f.recorder.state().started.len(), 1);
    assert_eq!(f.recorder.state().stop_count, 0);
    assert_eq!(f.coordinator.status().recording_trigger, Some(RecordingTrigger::Manual));

    f.coordinator.stop_manual();
    assert_eq!(f.recorder.state().stop_count, 1);
}

#[test]
fn manual_recording_without_a_meeting_targets_the_whole_display_and_default_devices() {
    let mut f = fixture();
    f.coordinator.start_manual();

    let state = f.recorder.state();
    let request = &state.started[0];
    assert!(request.output_path.ends_with("2026-11-17_10-00-00_Manual.mp4"));
    assert_eq!(request.video, VideoTarget::Display(None));
    assert_eq!(request.audio_process_id, None);
    assert_eq!(request.capture_device_id, None);
    assert_eq!(request.render_device_id, None);
}

#[test]
fn retries_with_a_delay_when_starting_fails() {
    let mut f = fixture();
    f.recorder
        .state()
        .start_failures
        .extend(["busy".to_owned(), "busy".to_owned()]);
    started(&mut f.coordinator);

    assert_eq!(f.recorder.state().started.len(), 1);
    assert_eq!(f.pauses.taken().len(), 2);
    assert!(f.coordinator.status().is_recording());
    assert!(!has_notice(&mut f.coordinator, NoticeLevel::Error));
}

#[test]
fn notifies_an_error_without_recording_when_start_retries_run_out() {
    let mut f = fixture();
    let failures = (0..Coordinator::MAX_START_ATTEMPTS).map(|_| "broken".to_owned());
    f.recorder.state().start_failures.extend(failures);
    started(&mut f.coordinator);

    assert!(f.recorder.state().started.is_empty());
    assert!(!f.coordinator.status().is_recording());
    assert!(has_notice(&mut f.coordinator, NoticeLevel::Error));

    ended(&mut f.coordinator);
    assert_eq!(f.recorder.state().stop_count, 0);
}

#[test]
fn resumes_in_a_different_file_when_recording_fails_during_a_meeting() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.recorder.fail("device lost");
    f.coordinator.poll_recorder();

    {
        let state = f.recorder.state();
        assert_eq!(state.started.len(), 2);
        assert_ne!(state.started[0].output_path, state.started[1].output_path);
    }
    assert!(f.coordinator.status().is_recording());
    assert!(has_notice(&mut f.coordinator, NoticeLevel::Warning));
}

#[test]
fn stops_resuming_and_notifies_an_error_when_recording_failures_repeat() {
    let mut f = fixture();
    started(&mut f.coordinator);
    for _ in 0..=Coordinator::MAX_RECOVERIES_PER_MEETING {
        f.recorder.fail("device lost");
        f.coordinator.poll_recorder();
    }

    assert_eq!(
        f.recorder.state().started.len() as u32,
        1 + Coordinator::MAX_RECOVERIES_PER_MEETING
    );
    assert!(!f.coordinator.status().is_recording());
    assert!(has_notice(&mut f.coordinator, NoticeLevel::Error));
}

#[test]
fn a_failed_manual_recording_is_not_resumed_and_notifies_an_error() {
    let mut f = fixture();
    f.coordinator.start_manual();
    f.recorder.fail("disk full");
    f.coordinator.poll_recorder();

    assert_eq!(f.recorder.state().started.len(), 1);
    assert!(!f.coordinator.status().is_recording());
    let notices = f.coordinator.take_notices();
    assert!(
        notices
            .iter()
            .any(|n| n.level == NoticeLevel::Error && n.message.contains("disk full"))
    );
}

#[test]
fn ignores_a_failure_that_arrives_after_stopping() {
    let mut f = fixture();
    started(&mut f.coordinator);
    ended(&mut f.coordinator);
    f.recorder.fail("late");
    f.coordinator.poll_recorder();

    assert_eq!(f.recorder.state().started.len(), 1);
    assert!(
        f.coordinator
            .take_notices()
            .iter()
            .all(|n| n.level == NoticeLevel::Info)
    );
}

#[test]
fn shutdown_stops_a_recording_in_progress() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.shutdown();

    assert_eq!(f.recorder.state().stop_count, 1);
    assert!(!f.coordinator.status().is_recording());
}

#[test]
fn a_file_name_can_be_made_even_if_the_rule_name_has_unusable_characters() {
    let mut f = fixture();
    let meeting = MeetingInfo {
        rule_name: "Web: Meet/Zoom".into(),
        ..zoom()
    };
    f.coordinator.handle(DetectorEvent::Started(meeting));

    let state = f.recorder.state();
    assert!(
        state.started[0]
            .output_path
            .ends_with("2026-11-17_10-00-00_Web--Meet-Zoom.mp4")
    );
}

#[test]
fn when_scope_is_screen_it_does_not_narrow_to_the_window_and_app_sound() {
    let f = fixture();
    let mut coordinator = screen_scope(&f);
    started(&mut coordinator);

    let state = f.recorder.state();
    let request = &state.started[0];
    assert_eq!(request.video, VideoTarget::Display(Some("DISPLAY2".into())));
    assert_eq!(request.audio_process_id, None);
    assert_eq!(request.render_device_id.as_deref(), Some("speaker"));
}

#[test]
fn manual_recording_during_a_meeting_targets_that_app() {
    let mut f = fixture();
    f.coordinator.set_auto_enabled(false);
    started(&mut f.coordinator);
    f.coordinator.start_manual();

    let state = f.recorder.state();
    assert_eq!(state.started[0].video, VideoTarget::Window(77));
    assert_eq!(state.started[0].audio_process_id, Some(100));
}

#[test]
fn the_window_being_recorded_can_be_switched() {
    let mut f = fixture();
    started(&mut f.coordinator);
    assert_eq!(f.coordinator.target_window(), Some(77));

    f.coordinator.retarget(88);

    assert_eq!(f.recorder.state().retargets, [88]);
    assert_eq!(f.coordinator.target_window(), Some(88));
    assert_eq!(f.recorder.state().started.len(), 1);
}

#[test]
fn switching_to_the_same_window_does_nothing() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.retarget(77);

    assert!(f.recorder.state().retargets.is_empty());
}

#[test]
fn when_switching_fails_the_target_stays_as_before() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.recorder.state().retarget_fails = true;
    f.coordinator.retarget(88);

    assert_eq!(f.coordinator.target_window(), Some(77));
    assert!(f.coordinator.status().is_recording());
}

#[test]
fn does_not_switch_windows_while_recording_the_whole_display() {
    let f = fixture();
    let mut coordinator = screen_scope(&f);
    started(&mut coordinator);
    coordinator.retarget(88);

    assert_eq!(coordinator.target_window(), None);
    assert!(f.recorder.state().retargets.is_empty());
}

#[test]
fn does_not_switch_windows_while_not_recording() {
    let mut f = fixture();
    f.coordinator.retarget(88);

    assert!(f.recorder.state().retargets.is_empty());
}

#[test]
fn when_no_app_window_is_found_it_starts_with_no_video_target_not_the_whole_display() {
    let mut f = fixture();
    let meeting = MeetingInfo { window: None, ..zoom() };
    f.coordinator.handle(DetectorEvent::Started(meeting));

    assert_eq!(f.recorder.state().started[0].video, VideoTarget::Blank);
    assert_eq!(f.recorder.state().started[0].audio_process_id, Some(100));
    assert!(f.coordinator.needs_window(&[77].into()));

    f.coordinator.retarget(88);
    assert_eq!(f.recorder.state().retargets, [88]);
    assert!(!f.coordinator.needs_window(&[88].into()));
}

#[test]
fn does_not_ask_for_another_window_while_the_target_window_remains() {
    let mut f = fixture();
    started(&mut f.coordinator);

    assert!(!f.coordinator.needs_window(&[77, 88].into()));
    assert!(f.coordinator.needs_window(&[88].into()));
}

fn meet_tab() -> MeetingInfo {
    MeetingInfo {
        rule_name: "GoogleMeet".into(),
        capture_device_id: None,
        render_device_id: None,
        process_id: None,
        window: None,
        display: None,
        browser_tab: Some(BrowserTab { link: 1, tab: 42 }),
    }
}

#[test]
fn a_meeting_reported_by_the_extension_is_recorded_in_its_tab() {
    let mut f = fixture();
    f.coordinator.handle(DetectorEvent::Started(meet_tab()));

    let state = f.recorder.state();
    let request = &state.started[0];
    assert_eq!(request.video, VideoTarget::BrowserTab(BrowserTab { link: 1, tab: 42 }));
    assert_eq!(request.audio_process_id, None);
    assert!(request.output_path.ends_with("2026-11-17_10-00-00_GoogleMeet.mp4"));
    // Tab recordings do not look for a window to move to.
    assert!(!f.coordinator.needs_window(&[].into()));
}

#[test]
fn when_scope_is_screen_an_extension_reported_meeting_is_recorded_as_the_whole_display() {
    let f = fixture();
    let mut coordinator = screen_scope(&f);
    coordinator.handle(DetectorEvent::Started(meet_tab()));

    assert_eq!(f.recorder.state().started[0].video, VideoTarget::Display(None));
}
