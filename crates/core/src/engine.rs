use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::config::{Config, ConfigError};
use crate::coordinator::{CoordinatorStatus, ManualTarget, Notice, NoticeLevel, RecordingCoordinator};
use crate::detector::{MeetingDetector, Timing};
use crate::recording::Recorder;
use crate::rules::{RuleEvaluator, RuleEvidence};
use crate::signals::{SignalSnapshot, SignalSource};

/// Ties together signal acquisition, rule evaluation, start/end detection, and recording coordination.
/// Call [`Engine::tick`] periodically. Use it from a single thread.
///
/// How signal acquisition failures are handled:
/// A transient failure is treated as "unknown" and changes nothing (so a recording is not stopped by mistake).
/// A failure lasting beyond `signal_outage_limit` is treated as "no signal" (so a recording cannot get stuck running).
pub struct Engine<S: SignalSource, R: Recorder> {
    source: S,
    evaluator: RuleEvaluator,
    detector: MeetingDetector,
    coordinator: RecordingCoordinator<R>,
    signal_outage_limit: Duration,
    failing_since: Option<Instant>,
    outage_reported: bool,
    /// Whether the windows were read at the previous tick.
    reading_windows: bool,
}

impl<S: SignalSource, R: Recorder> Engine<S, R> {
    pub const DEFAULT_SIGNAL_OUTAGE_LIMIT: Duration = Duration::from_secs(60);

    pub fn new(source: S, coordinator: RecordingCoordinator<R>, config: &Config) -> Result<Self, ConfigError> {
        Ok(Self {
            source,
            evaluator: RuleEvaluator::new(&config.rules)?,
            detector: MeetingDetector::new(
                Timing {
                    start_debounce: Duration::from_secs_f64(config.start_debounce_seconds),
                    end_grace: Duration::from_secs_f64(config.end_grace_seconds),
                },
                Timing {
                    start_debounce: Duration::from_secs_f64(config.browser_start_debounce_seconds),
                    end_grace: Duration::from_secs_f64(config.browser_end_grace_seconds),
                },
            ),
            coordinator,
            signal_outage_limit: Self::DEFAULT_SIGNAL_OUTAGE_LIMIT,
            failing_since: None,
            outage_reported: false,
            reading_windows: false,
        })
    }

    pub fn tick(&mut self, now: Instant) {
        self.coordinator.poll_recorder();

        // Hold the list of current windows only when they were read.
        let mut windows: Option<HashSet<u64>> = None;
        let evidence = match self.read_signals() {
            Ok((snapshot, windows_read)) => {
                if self.failing_since.take().is_some() {
                    log::info!("Signal acquisition recovered.");
                }
                self.outage_reported = false;
                if windows_read {
                    windows = Some(snapshot.windows.iter().map(|w| w.id).collect());
                }
                self.evaluator.evaluate(&snapshot)
            }
            Err(error) => {
                let failing_since = *self.failing_since.get_or_insert_with(|| {
                    log::error!("Cannot acquire the signals for meeting detection: {error:#}");
                    now
                });
                if now.duration_since(failing_since) < self.signal_outage_limit {
                    return;
                }
                if !self.outage_reported {
                    self.outage_reported = true;
                    log::error!("Signal acquisition keeps failing. Cannot detect meetings: {error:#}");
                    self.coordinator.notify(
                        NoticeLevel::Error,
                        "Meetings cannot be detected for an extended period. Check the log.".into(),
                    );
                }
                Vec::new()
            }
        };

        self.note_window_reading(windows.is_some(), &evidence);
        for event in self.detector.update(now, &evidence, windows.is_some()) {
            self.coordinator.handle(event);
        }
        if let Some(windows) = windows {
            self.follow_window(&windows, &evidence);
        }
    }

    /// Reads what this tick needs, and whether the windows were among it.
    ///
    /// The microphone sessions and the extension's reports are read on every tick. A rule can only be satisfied by
    /// an app that uses the microphone, so the processes (to find each session's app), the windows (to find the window
    /// that appears with its meeting) and the playback sessions (to find the app's output device) are read only when
    /// some process uses the microphone. While an app's meeting is detected or recorded, the processes and windows
    /// are read anyway: its window keeps it going when the app lets go of the microphone, and is followed when it
    /// closes. The windows found when reading begins count as open before any meeting.
    fn read_signals(&mut self) -> anyhow::Result<(SignalSnapshot, bool)> {
        let (browser_links, browser_meetings) = self.source.browser();
        let mut snapshot = SignalSnapshot {
            capture_sessions: self.source.capture_sessions()?,
            browser_links,
            browser_meetings,
            ..Default::default()
        };
        let microphone_in_use = !snapshot.capture_sessions.is_empty();
        let recording_an_app = self
            .coordinator
            .recorded_meeting()
            .is_some_and(|meeting| meeting.browser_tab.is_none());
        // A desktop meeting goes on while its window is open, also when the app has let go of the microphone.
        let app_meeting = self.detector.meetings().any(|meeting| meeting.browser_tab.is_none());
        if microphone_in_use {
            snapshot.render_sessions = self.source.render_sessions()?;
        }
        let windows_read = microphone_in_use || recording_an_app || app_meeting;
        if windows_read {
            snapshot.processes = self.source.processes()?;
            snapshot.windows = self.source.windows()?;
        }
        Ok((snapshot, windows_read))
    }

    /// Logs when the windows begin and stop being read, since the windows open when reading begins are never a
    /// meeting's.
    fn note_window_reading(&mut self, windows_read: bool, evidence: &[RuleEvidence]) {
        if windows_read == std::mem::replace(&mut self.reading_windows, windows_read) {
            return;
        }
        if windows_read {
            let open: usize = evidence.iter().map(|e| e.windows.len()).sum();
            log::info!("Looking at the windows: {open} of meeting apps are open and are not taken for a meeting's.");
        } else {
            log::info!("Stopped looking at the windows.");
        }
    }

    /// If the window being recorded was closed (or has not been decided yet), moves the recording to the meeting's
    /// window once it has one. While the target window remains, does not move even if another window appears. A
    /// meeting that is only waiting out its end grace keeps its last picture.
    fn follow_window(&mut self, windows: &HashSet<u64>, evidence: &[RuleEvidence]) {
        let Some(meeting) = self.coordinator.recorded_meeting().cloned() else {
            return;
        };
        if !self.detector.is_ongoing(&meeting, evidence) || !self.coordinator.needs_window(windows) {
            return;
        }
        // Only the meeting's own window, never another window of the app.
        let replacement = self
            .detector
            .meeting_window(&meeting)
            .filter(|window| windows.contains(window));
        if let Some(replacement) = replacement {
            self.coordinator.retarget(replacement);
        }
    }

    pub fn start_manual(&mut self, target: ManualTarget) {
        self.coordinator.start_manual(target);
    }

    pub fn stop_manual(&mut self) {
        self.coordinator.stop_manual();
    }

    pub fn set_auto_enabled(&mut self, enabled: bool) {
        self.coordinator.set_auto_enabled(enabled);
    }

    pub fn shutdown(&mut self) {
        self.coordinator.shutdown();
    }

    pub fn status(&self) -> CoordinatorStatus {
        self.coordinator.status()
    }

    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.coordinator.take_notices()
    }

    pub fn recorder(&self) -> &R {
        self.coordinator.recorder()
    }
}
