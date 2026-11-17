use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::CaptureScope;
use crate::detector::{DetectorEvent, MeetingInfo};
use crate::naming;
use crate::ports::CoordinatorPorts;
use crate::recording::{Recorder, RecordingRequest, VideoTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingTrigger {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// An event to tell the user about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorStatus {
    pub auto_enabled: bool,
    /// Rule name of the detected meeting.
    pub meeting: Option<String>,
    pub recording_trigger: Option<RecordingTrigger>,
    pub recording_path: Option<PathBuf>,
}

impl CoordinatorStatus {
    pub fn is_recording(&self) -> bool {
        self.recording_path.is_some()
    }
}

struct ActiveRecording {
    path: PathBuf,
    trigger: RecordingTrigger,
    /// The window targeted for video.
    window: Option<u64>,
    /// Whether this recording targets only the app's window for video (false for whole-display recordings).
    follows_app: bool,
}

/// Maps meeting start/end and manual actions to starting and stopping recordings.
///
/// - If auto-recording is enabled, starts recording when a meeting starts and stops when it ends.
/// - A manually started recording continues until stopped manually or until shutdown.
/// - If auto-recording is stopped manually, it does not resume for the rest of that meeting.
/// - Failures to start and failures during recording are retried a limited number of times.
///
/// It touches the outside world only through the recorder and [`CoordinatorPorts`] (clock, output folder, pause).
pub struct RecordingCoordinator<R: Recorder> {
    recorder: R,
    ports: CoordinatorPorts,
    capture_scope: CaptureScope,
    auto_enabled: bool,
    meeting: Option<MeetingInfo>,
    recording: Option<ActiveRecording>,
    recoveries: u32,
    notices: Vec<Notice>,
}

impl<R: Recorder> RecordingCoordinator<R> {
    pub const MAX_START_ATTEMPTS: u32 = 3;
    pub const MAX_RECOVERIES_PER_MEETING: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_secs(2);

    pub fn new(recorder: R, ports: CoordinatorPorts) -> Self {
        Self {
            recorder,
            ports,
            capture_scope: CaptureScope::default(),
            auto_enabled: true,
            meeting: None,
            recording: None,
            recoveries: 0,
            notices: Vec::new(),
        }
    }

    pub fn with_capture_scope(mut self, capture_scope: CaptureScope) -> Self {
        self.capture_scope = capture_scope;
        self
    }

    /// The window currently targeted for video while recording.
    pub fn target_window(&self) -> Option<u64> {
        self.recording.as_ref()?.window
    }

    /// True when recording targets an app window and that window is not in `existing`
    /// (it was closed, or has not been decided yet).
    pub fn needs_window(&self, existing: &HashSet<u64>) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|r| r.follows_app && r.window.is_none_or(|w| !existing.contains(&w)))
    }

    /// Moves the video target to another window of the same app.
    /// Does nothing unless recording targets an app window.
    pub fn retarget(&mut self, window: u64) {
        let Some(recording) = &mut self.recording else { return };
        if !recording.follows_app || recording.window == Some(window) {
            return;
        }
        match self.recorder.retarget(window) {
            Ok(()) => {
                log::info!("Switched the window being recorded.");
                recording.window = Some(window);
            }
            Err(error) => log::warn!("Could not switch the window being recorded: {error:#}"),
        }
    }

    pub fn status(&self) -> CoordinatorStatus {
        CoordinatorStatus {
            auto_enabled: self.auto_enabled,
            meeting: self.meeting.as_ref().map(|m| m.rule_name.clone()),
            recording_trigger: self.recording.as_ref().map(|r| r.trigger),
            recording_path: self.recording.as_ref().map(|r| r.path.clone()),
        }
    }

    /// Takes the notices accumulated since the previous call.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    pub fn recorder(&self) -> &R {
        &self.recorder
    }

    pub fn handle(&mut self, event: DetectorEvent) {
        match event {
            DetectorEvent::Started(meeting) => {
                log::info!("Meeting start detected: {}", meeting.rule_name);
                self.meeting = Some(meeting);
                self.recoveries = 0;
                if self.auto_enabled && self.recording.is_none() {
                    self.start(RecordingTrigger::Auto);
                }
            }
            DetectorEvent::Ended(meeting) => {
                log::info!("Meeting end detected: {}", meeting.rule_name);
                self.meeting = None;
                if self
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.trigger == RecordingTrigger::Auto)
                {
                    self.stop();
                }
            }
        }
    }

    pub fn start_manual(&mut self) {
        if self.recording.is_none() {
            self.start(RecordingTrigger::Manual);
        }
    }

    pub fn stop_manual(&mut self) {
        if self.recording.is_some() {
            self.stop();
        }
    }

    /// Toggles auto-recording. If re-enabled during a meeting, recording starts.
    /// Disabling it does not stop a recording in progress.
    pub fn set_auto_enabled(&mut self, enabled: bool) {
        self.auto_enabled = enabled;
        log::info!("auto-recording: {}", if enabled { "enabled" } else { "disabled" });
        if enabled && self.meeting.is_some() && self.recording.is_none() {
            self.start(RecordingTrigger::Auto);
        }
    }

    /// Stops the recording in progress and finalizes the file.
    pub fn shutdown(&mut self) {
        if self.recording.is_some() {
            self.stop();
        }
    }

    /// Checks whether the recorder failed during recording and resumes it if conditions allow. Call periodically.
    pub fn poll_recorder(&mut self) {
        let Some(error) = self.recorder.take_failure() else {
            return;
        };
        // A failure that arrives after stopping is ignored, since the user was already told the outcome.
        let Some(failed) = self.recording.take() else { return };
        log::error!("Recording was interrupted: {error} ({})", failed.path.display());

        let resume = failed.trigger == RecordingTrigger::Auto
            && self.auto_enabled
            && self.meeting.is_some()
            && self.recoveries < Self::MAX_RECOVERIES_PER_MEETING;
        if !resume {
            self.notify(NoticeLevel::Error, format!("Recording was interrupted: {error}"));
            return;
        }

        self.recoveries += 1;
        self.notify(
            NoticeLevel::Warning,
            "Recording was interrupted; resuming in a new file.".into(),
        );
        self.start(RecordingTrigger::Auto);
    }

    pub(crate) fn notify(&mut self, level: NoticeLevel, message: String) {
        self.notices.push(Notice { level, message });
    }

    fn start(&mut self, trigger: RecordingTrigger) {
        for attempt in 1..=Self::MAX_START_ATTEMPTS {
            match self.try_start() {
                Ok(request) => {
                    let path = request.output_path;
                    log::info!("Recording started ({trigger:?}): {}", path.display());
                    self.notify(NoticeLevel::Info, format!("Recording started: {}", file_name(&path)));
                    let (window, follows_app) = match request.video {
                        VideoTarget::Window(window) => (Some(window), true),
                        VideoTarget::Blank => (None, true),
                        VideoTarget::Display(_) | VideoTarget::BrowserTab(_) => (None, false),
                    };
                    self.recording = Some(ActiveRecording {
                        path,
                        trigger,
                        window,
                        follows_app,
                    });
                    return;
                }
                Err(error) => {
                    log::error!(
                        "Could not start recording ({attempt}/{}): {error:#}",
                        Self::MAX_START_ATTEMPTS
                    );
                    if attempt < Self::MAX_START_ATTEMPTS {
                        self.ports.pause.pause(Self::RETRY_DELAY);
                    }
                }
            }
        }
        self.notify(NoticeLevel::Error, "Could not start recording. Check the log.".into());
    }

    fn try_start(&mut self) -> anyhow::Result<RecordingRequest> {
        let label = self
            .meeting
            .as_ref()
            .map_or_else(|| "Manual".to_owned(), |m| m.rule_name.clone());
        let output_path = self.new_output_path(&label)?;
        let meeting = self.meeting.as_ref();
        // Recording only the app is possible only when the target app is known (while a meeting is detected).
        let app = meeting.filter(|_| self.capture_scope == CaptureScope::App);
        let video = match app {
            // With the app-only setting, never fall back to recording the whole display even if no window is found.
            Some(app) => match app.browser_tab {
                Some(tab) => VideoTarget::BrowserTab(tab),
                None => app.window.map_or(VideoTarget::Blank, VideoTarget::Window),
            },
            None => VideoTarget::Display(meeting.and_then(|m| m.display.clone())),
        };
        let request = RecordingRequest {
            output_path,
            video,
            audio_process_id: app.and_then(|m| m.process_id),
            render_device_id: meeting.and_then(|m| m.render_device_id.clone()),
            capture_device_id: meeting.and_then(|m| m.capture_device_id.clone()),
        };
        self.recorder.start(&request)?;
        Ok(request)
    }

    fn stop(&mut self) {
        let Some(recording) = self.recording.take() else { return };
        match self.recorder.stop() {
            Ok(()) => {
                log::info!("Recording saved: {}", recording.path.display());
                self.notify(
                    NoticeLevel::Info,
                    format!("Recording saved: {}", file_name(&recording.path)),
                );
            }
            Err(error) => {
                log::error!("Failed to stop recording: {} : {error:#}", recording.path.display());
                self.notify(
                    NoticeLevel::Error,
                    format!(
                        "Failed to stop recording. The file may be incomplete: {}",
                        file_name(&recording.path)
                    ),
                );
            }
        }
    }

    /// Prepares the output folder and picks a file location that is not yet in use.
    fn new_output_path(&mut self, label: &str) -> anyhow::Result<PathBuf> {
        self.ports.output.prepare()?;
        let stem = naming::recording_stem(self.ports.clock.now(), label);
        let file_name = naming::first_unused_file_name(&stem, |name| self.ports.output.contains(name));
        Ok(self.ports.output.path_of(&file_name))
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
