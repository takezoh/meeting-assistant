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

        for event in self.detector.update(now, &evidence) {
            self.coordinator.handle(event);
        }
        if let Some(windows) = windows {
            self.follow_window(&windows, &evidence);
        }
    }

    /// Reads what this tick needs, and whether the windows were among it.
    ///
    /// The microphone sessions and the extension's reports are read on every tick. A rule can only be satisfied by
    /// an app that uses the microphone, so the processes (to find each session's app), the windows (to choose the
    /// one to record) and the playback sessions (to find the app's output device) are read only when some process
    /// uses the microphone. While an app's meeting is recorded, the processes and windows are read anyway, to follow
    /// its window when it closes.
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
        if microphone_in_use {
            snapshot.render_sessions = self.source.render_sessions()?;
        }
        let windows_read = microphone_in_use || recording_an_app;
        if windows_read {
            snapshot.processes = self.source.processes()?;
            snapshot.windows = self.source.windows()?;
        }
        Ok((snapshot, windows_read))
    }

    /// If the window being recorded was closed (or has not been decided yet),
    /// moves the recording target to a window of the same app.
    /// While the target window remains, does not move even if a larger window appears.
    fn follow_window(&mut self, windows: &HashSet<u64>, evidence: &[RuleEvidence]) {
        let Some(meeting) = self.coordinator.recorded_meeting().cloned() else {
            return;
        };
        if !self.detector.meetings().any(|detected| detected.key() == meeting)
            || !self.coordinator.needs_window(windows)
        {
            return;
        }
        let replacement = evidence.iter().find(|e| e.key() == meeting).and_then(|e| e.window);
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
