use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::config::{Config, ConfigError};
use crate::coordinator::{CoordinatorStatus, ManualTarget, Notice, NoticeLevel, RecordingCoordinator};
use crate::detector::MeetingDetector;
use crate::recording::Recorder;
use crate::rules::{RuleEvaluator, RuleEvidence};
use crate::signals::SignalSource;

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
                Duration::from_secs_f64(config.start_debounce_seconds),
                Duration::from_secs_f64(config.end_grace_seconds),
            ),
            coordinator,
            signal_outage_limit: Self::DEFAULT_SIGNAL_OUTAGE_LIMIT,
            failing_since: None,
            outage_reported: false,
        })
    }

    pub fn tick(&mut self, now: Instant) {
        self.coordinator.poll_recorder();

        // Hold the list of current windows only when signals were acquired.
        let mut windows: Option<HashSet<u64>> = None;
        let evidence = match self.source.capture() {
            Ok(snapshot) => {
                if self.failing_since.take().is_some() {
                    log::info!("Signal acquisition recovered.");
                }
                self.outage_reported = false;
                windows = Some(snapshot.windows.iter().map(|w| w.id).collect());
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
