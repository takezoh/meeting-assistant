use meeting_assistant_core::fakes::{FakeOutputFolder, FakeRecorder, RecordedPauses, fake_ports_with, output_path};
use meeting_assistant_core::{
    BrowserTab, CaptureScope, CoordinatorStatus, DetectorEvent, ManualTarget, MeetingInfo, NoticeLevel,
    RecordingCoordinator, RecordingTrigger, VideoTarget,
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
        metadata: meeting_assistant_core::MeetingMetadata::default(),
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

fn trigger(status: &CoordinatorStatus) -> Option<RecordingTrigger> {
    status.recording.as_ref().map(|r| r.trigger)
}

fn record_zoom() -> ManualTarget {
    ManualTarget::Meeting(zoom().key())
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
    assert_eq!(trigger(&status), Some(RecordingTrigger::Auto));
    assert!(status.is_recording_meeting(&zoom().key()));
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
    let status = f.coordinator.status();
    assert_eq!(status.meetings.len(), 1);
    assert_eq!(status.meetings[0].key, zoom().key());
    assert!(!status.meetings[0].auto_stopped);
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
    f.coordinator.start_manual(ManualTarget::Screen);
    started(&mut f.coordinator);
    ended(&mut f.coordinator);

    assert_eq!(f.recorder.state().started.len(), 1);
    assert_eq!(f.recorder.state().stop_count, 0);
    assert_eq!(trigger(&f.coordinator.status()), Some(RecordingTrigger::Manual));

    f.coordinator.stop_manual();
    assert_eq!(f.recorder.state().stop_count, 1);
}

#[test]
fn a_manual_recording_of_a_meeting_continues_after_that_meeting_ends() {
    let mut f = fixture();
    f.coordinator.set_auto_enabled(false);
    started(&mut f.coordinator);
    f.coordinator.start_manual(record_zoom());
    ended(&mut f.coordinator);

    assert_eq!(f.recorder.state().stop_count, 0);
    let status = f.coordinator.status();
    assert!(status.meetings.is_empty());
    assert_eq!(trigger(&status), Some(RecordingTrigger::Manual));
}

#[test]
fn manual_recording_of_the_screen_targets_the_whole_display_and_default_devices() {
    let mut f = fixture();
    f.coordinator.start_manual(ManualTarget::Screen);

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
    f.coordinator.start_manual(ManualTarget::Screen);
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
    f.coordinator.start_manual(record_zoom());

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
        metadata: meeting_assistant_core::MeetingMetadata::default(),
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

fn teams() -> MeetingInfo {
    MeetingInfo {
        rule_name: "Teams".into(),
        process_id: Some(200),
        window: Some(88),
        ..zoom()
    }
}

#[test]
fn a_meeting_detected_while_another_is_recorded_waits_and_is_recorded_when_that_one_ends() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));

    let status = f.coordinator.status();
    let keys: Vec<_> = status.meetings.iter().map(|m| m.key.clone()).collect();
    assert_eq!(
        keys,
        [zoom().key(), teams().key()],
        "both are listed, in the order they started"
    );
    assert!(status.is_recording_meeting(&zoom().key()));
    assert_eq!(f.recorder.state().started.len(), 1, "one recording at a time");

    ended(&mut f.coordinator);

    let state = f.recorder.state();
    assert_eq!(state.stop_count, 1);
    assert_eq!(state.started.len(), 2);
    assert_eq!(state.started[1].video, VideoTarget::Window(88));
    assert_eq!(state.started[1].audio_process_id, Some(200));
}

#[test]
fn the_end_of_a_meeting_that_is_not_recorded_does_not_stop_the_recording() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    f.coordinator.handle(DetectorEvent::Ended(teams()));

    assert_eq!(f.recorder.state().stop_count, 0);
    assert!(f.coordinator.status().is_recording_meeting(&zoom().key()));
}

#[test]
fn stopping_an_automatic_recording_by_hand_marks_that_meeting_and_starts_no_other() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    f.coordinator.stop_manual();

    let status = f.coordinator.status();
    assert!(status.meetings[0].auto_stopped);
    assert!(!status.meetings[1].auto_stopped);
    assert!(!status.is_recording(), "the other detected meeting is not started");
    assert_eq!(f.recorder.state().started.len(), 1);

    // Nor does the end of the stopped meeting start it.
    ended(&mut f.coordinator);
    assert!(!f.coordinator.status().is_recording());
}

#[test]
fn after_a_stop_by_hand_the_next_meeting_that_starts_is_recorded() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    f.coordinator.stop_manual();

    let webex = MeetingInfo {
        rule_name: "Webex".into(),
        process_id: Some(300),
        window: Some(99),
        ..zoom()
    };
    f.coordinator.handle(DetectorEvent::Started(webex.clone()));

    let status = f.coordinator.status();
    assert!(
        status.is_recording_meeting(&webex.key()),
        "the meeting that started, not the one that was detected earlier"
    );
    assert_eq!(f.recorder.state().started[1].video, VideoTarget::Window(99));
}

#[test]
fn the_file_the_recorder_actually_writes_is_the_one_reported() {
    let mut f = fixture();
    f.recorder.state().written_extension = Some("webm".into());
    f.coordinator.handle(DetectorEvent::Started(meet_tab()));

    let expected = output_path("2026-11-17_10-00-00_GoogleMeet.webm");
    assert_eq!(f.coordinator.status().recording.unwrap().path, expected);
    let notices = f.coordinator.take_notices();
    assert!(notices[0].message.ends_with("GoogleMeet.webm"), "{notices:?}");

    f.coordinator.stop_manual();
    let saved = f.coordinator.take_notices();
    assert!(saved[0].message.ends_with("GoogleMeet.webm"), "{saved:?}");
}

#[test]
fn the_first_metadata_entry_tells_a_meeting_apart_in_the_status() {
    let mut f = fixture();
    let meet = MeetingInfo {
        metadata: meeting_assistant_core::MeetingMetadata::new([
            ("meeting_code", "abc-defg-hij"),
            ("page_title", "Meet - abc-defg-hij"),
        ]),
        ..meet_tab()
    };
    f.coordinator.handle(DetectorEvent::Started(meet));

    assert_eq!(
        f.coordinator.status().meetings[0].title.as_deref(),
        Some("abc-defg-hij")
    );
}

#[test]
fn a_recording_is_started_with_the_meetings_metadata_and_told_what_is_learned_later() {
    let mut f = fixture();
    let meeting = zoom();
    f.coordinator.handle(DetectorEvent::Started(meeting.clone()));
    assert_eq!(
        f.recorder.state().started[0].metadata,
        meeting_assistant_core::RecordingMetadata {
            app: "Zoom".into(),
            started_at: Some(meeting_assistant_core::fakes::FIXED_TIME),
            meeting: meeting_assistant_core::MeetingMetadata::default(),
        },
        "started at the time the file is named after"
    );

    let learned = MeetingInfo {
        metadata: meeting_assistant_core::MeetingMetadata::new([("conversation", "standup")]),
        ..meeting
    };
    f.coordinator.handle(DetectorEvent::Updated(learned));
    assert_eq!(
        f.recorder.state().described,
        [meeting_assistant_core::RecordingMetadata {
            app: "Zoom".into(),
            started_at: Some(meeting_assistant_core::fakes::FIXED_TIME),
            meeting: meeting_assistant_core::MeetingMetadata::new([("conversation", "standup")]),
        }],
        "the app and the start stay"
    );
    assert_eq!(f.coordinator.status().meetings[0].title.as_deref(), Some("standup"));
}

#[test]
fn a_detected_meeting_can_be_recorded_by_hand_after_its_automatic_recording_was_stopped() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.stop_manual();
    f.coordinator.start_manual(record_zoom());

    let status = f.coordinator.status();
    assert_eq!(trigger(&status), Some(RecordingTrigger::Manual));
    assert!(status.is_recording_meeting(&zoom().key()));
    assert_eq!(f.recorder.state().started[1].video, VideoTarget::Window(77));
}

#[test]
fn a_manual_start_while_recording_is_ignored() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    f.coordinator.start_manual(ManualTarget::Meeting(teams().key()));
    f.coordinator.start_manual(ManualTarget::Screen);

    assert_eq!(f.recorder.state().started.len(), 1);
    assert!(f.coordinator.status().is_recording_meeting(&zoom().key()));
}

#[test]
fn a_manual_start_for_a_meeting_that_is_no_longer_detected_records_nothing() {
    let mut f = fixture();
    f.coordinator.start_manual(record_zoom());

    assert!(f.recorder.state().started.is_empty());
    assert!(has_notice(&mut f.coordinator, NoticeLevel::Warning));
}

#[test]
fn when_a_meeting_cannot_be_started_automatically_the_next_detected_meeting_is_tried() {
    let mut f = fixture();
    f.coordinator.set_auto_enabled(false);
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    let failures = (0..Coordinator::MAX_START_ATTEMPTS).map(|_| "window gone".to_owned());
    f.recorder.state().start_failures.extend(failures);

    f.coordinator.set_auto_enabled(true);

    let status = f.coordinator.status();
    assert!(status.meetings[0].auto_stopped);
    assert!(status.is_recording_meeting(&teams().key()));
}

#[test]
fn re_enabling_auto_recording_clears_meetings_whose_automatic_recording_was_stopped() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.stop_manual();
    f.coordinator.set_auto_enabled(false);
    f.coordinator.set_auto_enabled(true);

    let status = f.coordinator.status();
    assert!(!status.meetings[0].auto_stopped);
    assert_eq!(trigger(&status), Some(RecordingTrigger::Auto));
}

#[test]
fn shutdown_does_not_start_a_waiting_meeting() {
    let mut f = fixture();
    started(&mut f.coordinator);
    f.coordinator.handle(DetectorEvent::Started(teams()));
    f.coordinator.shutdown();

    assert_eq!(f.recorder.state().started.len(), 1);
    assert!(!f.coordinator.status().is_recording());
}

#[test]
fn when_scope_is_screen_an_extension_reported_meeting_is_recorded_as_the_whole_display() {
    let f = fixture();
    let mut coordinator = screen_scope(&f);
    coordinator.handle(DetectorEvent::Started(meet_tab()));

    assert_eq!(f.recorder.state().started[0].video, VideoTarget::Display(None));
}
