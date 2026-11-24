use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::CaptureScope;
use crate::detector::{DetectorEvent, MeetingInfo, MeetingKey};
use crate::handover::{self, Group};
use crate::metadata::RecordingMetadata;
use crate::naming;
use crate::ports::{CoordinatorPorts, LocalDateTime};
use crate::recording::{MissingSound, Recorder, RecordingRequest, Sound, VideoTarget};
use crate::target::RecordingTarget;

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
    pub target: RecordingTarget,
}

impl RecordingStatus {
    /// The meeting being recorded. `None` for a recording of the desktop or a window.
    pub fn meeting(&self) -> Option<&MeetingKey> {
        self.target.meeting()
    }
}

impl CoordinatorStatus {
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Whether this meeting is the one being recorded.
    pub fn is_recording_meeting(&self, key: &MeetingKey) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|recording| recording.meeting() == Some(key))
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
    target: RecordingTarget,
    /// The window targeted for video.
    window: Option<u64>,
    /// Whether the video follows the meeting app's window: false for display recordings and for a window chosen by
    /// hand, which is never exchanged for another.
    follows_app: bool,
    /// What the recording was last told to write into its file.
    metadata: RecordingMetadata,
}

impl ActiveRecording {
    fn meeting(&self) -> Option<&MeetingKey> {
        self.target.meeting()
    }
}

/// Maps meeting start/end and manual actions to starting and stopping recordings.
///
/// - Several meetings can be detected at once, but at most one recording exists.
/// - If auto-recording is enabled, a meeting that starts while nothing is being recorded is recorded, and the
///   recording stops when that meeting ends. Then the earliest meeting that started meanwhile, and whose automatic
///   recording has not been stopped, is recorded next.
/// - A manually started recording, of a meeting, the desktop or a window, continues until stopped manually or until
///   shutdown.
/// - Stopping a recording by hand starts nothing else, and auto-recording does not start that meeting again for the
///   rest of the meeting.
/// - Failures to start and failures during recording are retried a limited number of times.
/// - A recording is written in the output folder's in-progress folder. Once the recorder has stopped writing it, also
///   after a failure, its files are moved into the recordings folder; what an earlier run left in the in-progress
///   folder is moved by [`Self::recover_unfinished`].
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
        self.recording.as_ref()?.meeting()
    }

    /// True when recording targets an app window and that window is not in `existing`
    /// (it was closed, or has not been decided yet).
    pub fn needs_window(&self, existing: &HashSet<u64>) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|r| r.follows_app && r.window.is_none_or(|w| !existing.contains(&w)))
    }

    /// Moves the video target to the given window of the recorded app, which the caller has decided is the meeting's.
    /// Does nothing unless recording targets an app window.
    pub fn retarget(&mut self, window: u64) {
        let Some(recording) = &mut self.recording else { return };
        if !recording.follows_app || recording.window == Some(window) {
            return;
        }
        match self.recorder.retarget(window) {
            Ok(()) => {
                log::info!("Asked the recording to move to window {window:#x}.");
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
                    title: meeting.info.metadata.label().map(str::to_owned),
                    auto_stopped: meeting.auto_stopped,
                })
                .collect(),
            recording: self.recording.as_ref().map(|recording| RecordingStatus {
                path: recording.path.clone(),
                trigger: recording.trigger,
                target: recording.target.clone(),
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
                    && !self.start(RecordingTrigger::Auto, RecordingTarget::Meeting(key.clone()))
                {
                    self.mark_auto_stopped(&key);
                }
            }
            DetectorEvent::Updated(meeting) => {
                let key = meeting.key();
                if let Some(recording) = self.recording.as_mut().filter(|r| r.meeting() == Some(&key)) {
                    // The app and the start stay as they were; only what is known of the meeting changes.
                    recording.metadata.meeting = meeting.metadata.clone();
                    self.recorder.describe(&recording.metadata);
                }
                if let Some(known) = self.meetings.iter_mut().find(|m| m.info.key() == key) {
                    known.info = meeting;
                }
            }
            DetectorEvent::Ended(meeting) => {
                log::info!("Meeting end detected: {}", meeting.rule_name);
                let key = meeting.key();
                self.meetings.retain(|m| m.info.key() != key);
                if self
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.trigger == RecordingTrigger::Auto && r.meeting() == Some(&key))
                {
                    self.stop();
                    // The automatic recording passes to a meeting that waited for it.
                    self.record_automatically();
                }
            }
        }
    }

    /// Starts a recording by hand: of a detected meeting, recorded as its automatic recording would be, of the desktop
    /// or of a window. Does nothing while something is being recorded.
    pub fn start_manual(&mut self, target: RecordingTarget) {
        if self.recording.is_some() {
            log::info!("Already recording; the manual start was ignored.");
            return;
        }
        if target.meeting().is_some_and(|key| self.meeting(key).is_none()) {
            self.notify(NoticeLevel::Warning, "That meeting is no longer detected.".into());
            return;
        }
        self.start(RecordingTrigger::Manual, target);
    }

    /// Stops the recording in progress. A meeting whose recording is stopped is not recorded automatically again,
    /// and stopping does not start a recording of another detected meeting.
    pub fn stop_manual(&mut self) {
        let Some(recording) = &self.recording else { return };
        if let Some(key) = recording.meeting().cloned() {
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

    /// Moves into the recordings folder what an earlier run left in the in-progress folder, for example when it was
    /// killed while recording. Call once at startup, before anything is recorded.
    pub fn recover_unfinished(&mut self) {
        let names = match self.ports.output.in_progress() {
            Ok(names) => names,
            Err(error) => {
                log::warn!("Could not look for recordings left in progress by an earlier run: {error:#}");
                return;
            }
        };
        for group in handover::groups(&names) {
            let Some(moved) = self.hand_over(&group) else { continue };
            log::warn!(
                "Moved a recording left in progress by an earlier run: {}",
                moved.display()
            );
            self.notify(
                NoticeLevel::Warning,
                format!(
                    "A recording left unfinished by an earlier run was moved to the recordings folder: {}",
                    file_name(&moved)
                ),
            );
        }
    }

    /// Checks whether the recorder failed during recording and resumes it if conditions allow. Call periodically.
    pub fn poll_recorder(&mut self) {
        let Some(error) = self.recorder.take_failure() else {
            return;
        };
        // A failure that arrives after stopping is ignored, since the user was already told the outcome.
        let Some(failed) = self.recording.take() else { return };
        // The recorder no longer writes the failed file, which keeps what was written.
        let kept = self.hand_over_written(&failed.path);
        log::error!("Recording was interrupted: {error} ({})", kept.display());

        let auto_enabled = self.auto_enabled;
        let meeting = failed.meeting().cloned().and_then(|key| self.meeting_mut(&key));
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
            let key = failed.meeting().cloned();
            if self.start(RecordingTrigger::Auto, failed.target) {
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
            if !self.start(RecordingTrigger::Auto, RecordingTarget::Meeting(key.clone())) {
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
    fn start(&mut self, trigger: RecordingTrigger, target: RecordingTarget) -> bool {
        for attempt in 1..=Self::MAX_START_ATTEMPTS {
            match self.try_start(&target) {
                Ok((request, missing)) => {
                    let path = request.output_path;
                    let metadata = request.metadata;
                    log::info!("Recording started ({trigger:?}): {}", path.display());
                    self.notify(NoticeLevel::Info, format!("Recording started: {}", file_name(&path)));
                    // A silent track is noticed only when the recording is played, so it is told now.
                    for missing in &missing {
                        self.notify(NoticeLevel::Warning, missing_sound_notice(missing));
                    }
                    // Only a meeting's window is followed; a window chosen by hand stays the one recorded.
                    let follows_meeting = target.meeting().is_some();
                    let (window, follows_app) = match request.video {
                        VideoTarget::Window(window) => (Some(window), follows_meeting),
                        VideoTarget::Blank => (None, follows_meeting),
                        VideoTarget::Display(_) | VideoTarget::BrowserTab(_) => (None, false),
                    };
                    self.recording = Some(ActiveRecording {
                        path,
                        trigger,
                        target,
                        window,
                        follows_app,
                        metadata,
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

    /// Starts recording `target`. Returns the request as the recorder carried it out, with the sounds it does not
    /// record.
    fn try_start(&mut self, target: &RecordingTarget) -> anyhow::Result<(RecordingRequest, Vec<MissingSound>)> {
        // A meeting that is gone must not turn into a recording of the screen.
        let meeting = match target.meeting() {
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
        let started_at = self.ports.clock.now();
        let output_path = self.new_output_path(started_at, &label)?;
        let meeting = meeting.as_ref();
        // Recording only the app is possible only when the target app is known (a detected meeting).
        let app = meeting.filter(|_| self.capture_scope == CaptureScope::App);
        let (video, audio_process_id) = match (target, app) {
            // A window chosen by hand, with its app's sound and never all output audio.
            (RecordingTarget::Window { window, .. }, _) => (VideoTarget::Window(window.id), Some(window.process_id)),
            // With the app-only setting, never fall back to recording the whole display even if no window is found.
            (_, Some(app)) => (
                match app.browser_tab {
                    Some(tab) => VideoTarget::BrowserTab(tab),
                    None => app.window.map_or(VideoTarget::Blank, VideoTarget::Window),
                },
                app.process_id,
            ),
            (_, None) => (VideoTarget::Display(meeting.and_then(|m| m.display.clone())), None),
        };
        let request = RecordingRequest {
            output_path,
            video,
            audio_process_id,
            render_device_id: meeting.and_then(|m| m.render_device_id.clone()),
            capture_device_id: meeting.and_then(|m| m.capture_device_id.clone()),
            microphone: target.microphone(),
            metadata: RecordingMetadata {
                app: label,
                started_at: Some(started_at),
                meeting: meeting.map(|m| m.metadata.clone()).unwrap_or_default(),
            },
        };
        // Report the file that is actually written; a tab recorded as WebM gets another extension.
        // What a failed start began is not moved: a start that did not finish in time may still be creating and
        // writing its file. It stays in progress, and the next start of the app moves it.
        let started = self.recorder.start(&request)?;
        Ok((
            RecordingRequest {
                output_path: started.path,
                ..request
            },
            started.missing,
        ))
    }

    fn stop(&mut self) {
        let Some(recording) = self.recording.take() else { return };
        let stopped = self.recorder.stop();
        // Stopping returns once nothing writes the file any more, whether it succeeded or not.
        let saved = self.hand_over_written(&recording.path);
        match stopped {
            Ok(()) => {
                log::info!("Recording saved: {}", saved.display());
                self.notify(NoticeLevel::Info, format!("Recording saved: {}", file_name(&saved)));
            }
            Err(error) => {
                log::error!("Failed to stop recording: {} : {error:#}", saved.display());
                self.notify(
                    NoticeLevel::Error,
                    format!(
                        "Failed to stop recording. The file may be incomplete: {}",
                        file_name(&saved)
                    ),
                );
            }
        }
    }

    /// Moves the recording written at `written`, with the files written beside it, into the recordings folder.
    /// Returns where the recording is now: in the recordings folder, or still at `written` if it could not be moved.
    fn hand_over_written(&mut self, written: &Path) -> PathBuf {
        let Some(name) = written.file_name().and_then(|name| name.to_str()) else {
            return written.to_path_buf();
        };
        let stem = handover::stem_of(name).to_owned();
        match self.ports.output.in_progress() {
            Ok(names) => {
                let group = handover::group(&stem, &names);
                self.hand_over(&group).unwrap_or_else(|| written.to_path_buf())
            }
            Err(error) => {
                log::error!("Could not read the in-progress folder: {error:#}");
                self.notify(
                    NoticeLevel::Error,
                    format!(
                        "Could not move the recording to the recordings folder; it is still at {}",
                        written.display()
                    ),
                );
                written.to_path_buf()
            }
        }
    }

    /// Moves the files of `group` into the recordings folder, the recording's own file last. Returns the recording's
    /// new location, or where the first file went for a group without one; `None` when nothing was moved.
    fn hand_over(&mut self, group: &Group) -> Option<PathBuf> {
        let plan = handover::handover(group, |name| self.ports.output.contains(name));
        for name in &plan.discard {
            if let Err(error) = self.ports.output.discard(name) {
                log::warn!("Could not remove the unfinished copy {name}: {error:#}");
            }
        }
        let mut moved: Option<PathBuf> = None;
        for step in &plan.moves {
            match self.ports.output.publish(&step.from, &step.to) {
                Ok(path) => {
                    if moved.is_none() || group.recording.as_ref() == Some(&step.from) {
                        moved = Some(path);
                    }
                }
                Err(error) => {
                    log::error!("Could not move {} to the recordings folder: {error:#}", step.from);
                    self.notify(
                        NoticeLevel::Error,
                        format!(
                            "Could not move {} to the recordings folder; it is still in the in-progress folder.",
                            step.from
                        ),
                    );
                }
            }
        }
        moved
    }

    /// Prepares the output folder and picks a name used neither in the recordings folder nor in progress. Returns
    /// where that recording is written while in progress.
    fn new_output_path(&mut self, started_at: LocalDateTime, label: &str) -> anyhow::Result<PathBuf> {
        self.ports.output.prepare()?;
        let stem = naming::recording_stem(started_at, label);
        let in_progress = self.ports.output.in_progress()?;
        let output = &self.ports.output;
        let file_name = naming::first_unused_file_name(&stem, |name| {
            output.contains(name) || !handover::group(handover::stem_of(name), &in_progress).files.is_empty()
        });
        Ok(self.ports.output.in_progress_path(&file_name))
    }
}

/// What the user is told of a sound a recording does not have.
fn missing_sound_notice(missing: &MissingSound) -> String {
    let what = match missing.sound {
        Sound::OtherSide => "The other participants are not being recorded; their track stays silent",
        Sound::Microphone => "Your microphone is not being recorded; its track stays silent",
    };
    format!("{what}: {}", missing.reason)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
