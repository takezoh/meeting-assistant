use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::CaptureScope;
use crate::detector::{DetectorEvent, MeetingInfo, MeetingKey};
use crate::naming;
use crate::ports::CoordinatorPorts;
use crate::recording::{Recorder, RecordingRequest, VideoTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingTrigger {
    Auto,
    Manual,
}

/// What a manual recording records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualTarget {
    /// One detected meeting, recorded as its automatic recording would be.
    Meeting(MeetingKey),
    /// The main display and all output audio, with no meeting attached.
    Screen,
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

/// What the user can see: the detected meetings and the recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorStatus {
    pub auto_enabled: bool,
    /// Detected meetings, in the order they started.
    pub meetings: Vec<MeetingStatus>,
    pub recording: Option<RecordingStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingStatus {
    pub key: MeetingKey,
    /// What tells the meeting apart from others of the same rule, when known.
    pub title: Option<String>,
    /// Automatic recording was stopped by hand or gave up for this meeting, so it does not start it again.
    pub auto_stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingStatus {
    pub path: PathBuf,
    pub trigger: RecordingTrigger,
    /// The meeting being recorded. `None` for a recording of the screen.
    pub meeting: Option<MeetingKey>,
}

impl CoordinatorStatus {
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Whether this meeting is the one being recorded.
    pub fn is_recording_meeting(&self, key: &MeetingKey) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|recording| recording.meeting.as_ref() == Some(key))
    }
}

struct Meeting {
    info: MeetingInfo,
    auto_stopped: bool,
    /// How many times a failed recording of this meeting was started again.
    recoveries: u32,
}

struct ActiveRecording {
    path: PathBuf,
    trigger: RecordingTrigger,
    meeting: Option<MeetingKey>,
    /// The window targeted for video.
    window: Option<u64>,
    /// Whether this recording targets only the app's window for video (false for whole-display recordings).
    follows_app: bool,
}

/// Maps meeting start/end and manual actions to starting and stopping recordings.
///
/// - Several meetings can be detected at once, but at most one recording exists.
/// - If auto-recording is enabled, a meeting that starts while nothing is being recorded is recorded, and the
///   recording stops when that meeting ends. Then the earliest meeting that started meanwhile, and whose automatic
///   recording has not been stopped, is recorded next.
/// - A manually started recording, of a meeting or of the screen, continues until stopped manually or until shutdown.
/// - Stopping a recording by hand starts nothing else, and auto-recording does not start that meeting again for the
///   rest of the meeting.
/// - Failures to start and failures during recording are retried a limited number of times.
///
/// It touches the outside world only through the recorder and [`CoordinatorPorts`] (clock, output folder, pause).
pub struct RecordingCoordinator<R: Recorder> {
    recorder: R,
    ports: CoordinatorPorts,
    capture_scope: CaptureScope,
    auto_enabled: bool,
    meetings: Vec<Meeting>,
    recording: Option<ActiveRecording>,
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
            meetings: Vec::new(),
            recording: None,
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

    /// The meeting being recorded.
    pub fn recorded_meeting(&self) -> Option<&MeetingKey> {
        self.recording.as_ref()?.meeting.as_ref()
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
            meetings: self
                .meetings
                .iter()
                .map(|meeting| MeetingStatus {
                    key: meeting.info.key(),
                    title: meeting.info.title.clone(),
                    auto_stopped: meeting.auto_stopped,
                })
                .collect(),
            recording: self.recording.as_ref().map(|recording| RecordingStatus {
                path: recording.path.clone(),
                trigger: recording.trigger,
                meeting: recording.meeting.clone(),
            }),
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
                let key = meeting.key();
                self.meetings.push(Meeting {
                    info: meeting,
                    auto_stopped: false,
                    recoveries: 0,
                });
                // A meeting that starts while nothing is recorded is recorded. One that starts during a recording
                // waits for the meeting being recorded automatically to end.
                if self.auto_enabled
                    && self.recording.is_none()
                    && !self.start(RecordingTrigger::Auto, Some(key.clone()))
                {
                    self.mark_auto_stopped(&key);
                }
            }
            DetectorEvent::Ended(meeting) => {
                log::info!("Meeting end detected: {}", meeting.rule_name);
                let key = meeting.key();
                self.meetings.retain(|m| m.info.key() != key);
                if self
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.trigger == RecordingTrigger::Auto && r.meeting.as_ref() == Some(&key))
                {
                    self.stop();
                    // The automatic recording passes to a meeting that waited for it.
                    self.record_automatically();
                }
            }
        }
    }

    /// Starts a recording by hand. Does nothing while something is being recorded.
    pub fn start_manual(&mut self, target: ManualTarget) {
        if self.recording.is_some() {
            log::info!("Already recording; the manual start was ignored.");
            return;
        }
        let meeting = match target {
            ManualTarget::Screen => None,
            ManualTarget::Meeting(key) if self.meeting(&key).is_none() => {
                self.notify(NoticeLevel::Warning, "That meeting is no longer detected.".into());
                return;
            }
            ManualTarget::Meeting(key) => Some(key),
        };
        self.start(RecordingTrigger::Manual, meeting);
    }

    /// Stops the recording in progress. A meeting whose recording is stopped is not recorded automatically again,
    /// and stopping does not start a recording of another detected meeting.
    pub fn stop_manual(&mut self) {
        let Some(recording) = &self.recording else { return };
        if let Some(key) = recording.meeting.clone() {
            self.mark_auto_stopped(&key);
        }
        self.stop();
    }

    /// Toggles auto-recording. If enabled during a meeting, recording starts, including for meetings whose automatic
    /// recording was stopped earlier. Disabling it does not stop a recording in progress.
    pub fn set_auto_enabled(&mut self, enabled: bool) {
        self.auto_enabled = enabled;
        log::info!("auto-recording: {}", if enabled { "enabled" } else { "disabled" });
        if enabled {
            for meeting in &mut self.meetings {
                meeting.auto_stopped = false;
            }
            self.record_automatically();
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

        let auto_enabled = self.auto_enabled;
        let meeting = failed.meeting.as_ref().and_then(|key| self.meeting_mut(key));
        let resume = match meeting {
            Some(meeting)
                if failed.trigger == RecordingTrigger::Auto
                    && auto_enabled
                    && meeting.recoveries < Self::MAX_RECOVERIES_PER_MEETING =>
            {
                meeting.recoveries += 1;
                true
            }
            Some(meeting) => {
                meeting.auto_stopped = true;
                false
            }
            None => false,
        };
        if resume {
            self.notify(
                NoticeLevel::Warning,
                "Recording was interrupted; resuming in a new file.".into(),
            );
            let key = failed.meeting.clone();
            if self.start(RecordingTrigger::Auto, failed.meeting) {
                return;
            }
            if let Some(key) = key {
                self.mark_auto_stopped(&key);
            }
        } else {
            self.notify(NoticeLevel::Error, format!("Recording was interrupted: {error}"));
        }
        self.record_automatically();
    }

    pub(crate) fn notify(&mut self, level: NoticeLevel, message: String) {
        self.notices.push(Notice { level, message });
    }

    /// If auto-recording is enabled and nothing is being recorded, records the earliest meeting it may record.
    fn record_automatically(&mut self) {
        while self.auto_enabled && self.recording.is_none() {
            let Some(key) = self.meetings.iter().find(|m| !m.auto_stopped).map(|m| m.info.key()) else {
                return;
            };
            if !self.start(RecordingTrigger::Auto, Some(key.clone())) {
                // Do not try the same meeting again on every event; another detected meeting may still start.
                self.mark_auto_stopped(&key);
            }
        }
    }

    fn meeting(&self, key: &MeetingKey) -> Option<&Meeting> {
        self.meetings.iter().find(|m| m.info.key() == *key)
    }

    fn meeting_mut(&mut self, key: &MeetingKey) -> Option<&mut Meeting> {
        self.meetings.iter_mut().find(|m| m.info.key() == *key)
    }

    fn mark_auto_stopped(&mut self, key: &MeetingKey) {
        if let Some(meeting) = self.meeting_mut(key) {
            meeting.auto_stopped = true;
        }
    }

    /// Returns whether the recording started.
    fn start(&mut self, trigger: RecordingTrigger, meeting: Option<MeetingKey>) -> bool {
        for attempt in 1..=Self::MAX_START_ATTEMPTS {
            match self.try_start(meeting.as_ref()) {
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
                        meeting,
                        window,
                        follows_app,
                    });
                    return true;
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
        false
    }

    fn try_start(&mut self, meeting: Option<&MeetingKey>) -> anyhow::Result<RecordingRequest> {
        // A meeting that is gone must not turn into a recording of the screen.
        let meeting = match meeting {
            Some(key) => Some(
                self.meeting(key)
                    .map(|m| m.info.clone())
                    .ok_or_else(|| anyhow::anyhow!("the meeting {} is no longer detected", key.rule_name))?,
            ),
            None => None,
        };
        let label = meeting
            .as_ref()
            .map_or_else(|| "Manual".to_owned(), |m| m.rule_name.clone());
        let output_path = self.new_output_path(&label)?;
        let meeting = meeting.as_ref();
        // Recording only the app is possible only when the target app is known (a detected meeting).
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
        // Report the file that is actually written; a tab recorded as WebM gets another extension.
        let written = self.recorder.start(&request)?;
        Ok(RecordingRequest {
            output_path: written,
            ..request
        })
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
