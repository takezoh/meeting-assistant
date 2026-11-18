//! The thread that runs detection and recording. It talks to the UI side only through messages.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use meeting_assistant_core::{CoordinatorStatus, Engine, ManualTarget, Notice, Recorder, SignalSource};

/// Instructions from the UI side.
pub enum Command {
    StartManual(ManualTarget),
    StopManual,
    SetAutoEnabled(bool),
    /// Finalizes the recording and exits. Sends an acknowledgement back when done.
    Shutdown(mpsc::Sender<()>),
}

/// What is reported to the UI side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Status(CoordinatorStatus),
    Notice(Notice),
}

/// Advances detection every `interval` and handles instructions in between.
/// Continues until a quit instruction arrives or the sender of instructions is gone.
/// Calls `publish` on every state change and every notification.
pub fn run<S: SignalSource, R: Recorder>(
    mut engine: Engine<S, R>,
    interval: Duration,
    commands: &mpsc::Receiver<Command>,
    publish: impl Fn(Update),
) {
    let mut next_tick = Instant::now();
    let mut last_status = engine.status();
    publish(Update::Status(last_status.clone()));

    loop {
        // Where to send the completion acknowledgement when quitting (`None` if there is no one to notify).
        let mut finished: Option<Option<mpsc::Sender<()>>> = None;
        match commands.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(Command::StartManual(target)) => engine.start_manual(target),
            Ok(Command::StopManual) => engine.stop_manual(),
            Ok(Command::SetAutoEnabled(enabled)) => engine.set_auto_enabled(enabled),
            Ok(Command::Shutdown(done)) => {
                engine.shutdown();
                finished = Some(Some(done));
            }
            Err(RecvTimeoutError::Timeout) => {
                engine.tick(Instant::now());
                // If processing runs long, do not run the accumulated ticks back to back.
                next_tick = (next_tick + interval).max(Instant::now());
            }
            Err(RecvTimeoutError::Disconnected) => {
                engine.shutdown();
                finished = Some(None);
            }
        }

        for notice in engine.take_notices() {
            publish(Update::Notice(notice));
        }
        let status = engine.status();
        if status != last_status {
            last_status = status.clone();
            publish(Update::Status(status));
        }

        if let Some(done) = finished {
            if let Some(done) = done {
                let _ = done.send(());
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use meeting_assistant_core::fakes::{FakeRecorder, ScriptedSignals, fake_ports, zoom_in_meeting};
    use meeting_assistant_core::{Config, NoticeLevel, RecordingCoordinator, RecordingTrigger};

    use super::*;

    const INTERVAL: Duration = Duration::from_millis(5);
    const PATIENCE: Duration = Duration::from_secs(10);

    struct Harness {
        commands: mpsc::Sender<Command>,
        updates: Arc<Mutex<Vec<Update>>>,
        recorder: FakeRecorder,
        signals: ScriptedSignals,
        thread: std::thread::JoinHandle<()>,
    }

    /// Runs an engine built only from fakes on another thread. The start and end grace periods are 0 seconds.
    fn harness() -> Harness {
        let (recorder, signals) = (FakeRecorder::default(), ScriptedSignals::default());
        let config = Config {
            start_debounce_seconds: 0.0,
            end_grace_seconds: 0.0,
            ..Config::default()
        };
        let coordinator = RecordingCoordinator::new(recorder.clone(), fake_ports(&recorder));
        let engine = Engine::new(signals.clone(), coordinator, &config).unwrap();

        let (commands, receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(Vec::new()));
        let thread = std::thread::spawn({
            let updates = updates.clone();
            move || {
                run(engine, INTERVAL, &receiver, |update| {
                    updates.lock().unwrap().push(update)
                })
            }
        });
        let harness = Harness {
            commands,
            updates,
            recorder,
            signals,
            thread,
        };
        // The loop first reports the current state. Later tests are written assuming that has arrived.
        harness.wait_until("first status arrives", |h| !h.updates.lock().unwrap().is_empty());
        harness
    }

    impl Harness {
        /// Waits until the condition holds. Fails if it never does.
        fn wait_until(&self, what: &str, condition: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + PATIENCE;
            while !condition(self) {
                assert!(Instant::now() < deadline, "{what} never happened");
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn latest_status(&self) -> CoordinatorStatus {
            let updates = self.updates.lock().unwrap();
            updates
                .iter()
                .rev()
                .find_map(|u| {
                    if let Update::Status(s) = u {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap()
        }

        fn shut_down(self) -> Vec<Update> {
            let (done, acknowledged) = mpsc::channel();
            self.commands.send(Command::Shutdown(done)).unwrap();
            acknowledged
                .recv_timeout(PATIENCE)
                .expect("completion of quit is acknowledged");
            self.thread.join().unwrap();
            Arc::try_unwrap(self.updates).unwrap().into_inner().unwrap()
        }
    }

    #[test]
    fn reports_the_current_status_first() {
        let h = harness();
        h.wait_until("status arrives", |h| !h.updates.lock().unwrap().is_empty());

        let status = h.latest_status();
        assert!(status.auto_enabled && !status.is_recording());
        h.shut_down();
    }

    #[test]
    fn advances_detection_each_tick_and_records_and_reports_when_a_meeting_starts() {
        let h = harness();
        h.signals.set(zoom_in_meeting());
        h.wait_until("recording", |h| h.recorder.state().recording);
        h.wait_until("recording status arrives", |h| h.latest_status().is_recording());

        assert_eq!(
            h.latest_status().recording.map(|r| r.trigger),
            Some(RecordingTrigger::Auto)
        );
        let updates = h.shut_down();
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, Update::Notice(n) if n.level == NoticeLevel::Info))
        );
    }

    #[test]
    fn starts_and_stops_recording_on_manual_instruction() {
        let h = harness();
        h.commands.send(Command::StartManual(ManualTarget::Screen)).unwrap();
        h.wait_until("recording", |h| h.recorder.state().recording);
        h.wait_until("manual recording status arrives", |h| {
            h.latest_status()
                .recording
                .is_some_and(|r| r.trigger == RecordingTrigger::Manual)
        });

        h.commands.send(Command::StopManual).unwrap();
        h.wait_until("stopped", |h| !h.recorder.state().recording);
        h.shut_down();
    }

    #[test]
    fn does_not_record_when_a_meeting_starts_while_auto_recording_is_disabled() {
        let h = harness();
        h.commands.send(Command::SetAutoEnabled(false)).unwrap();
        h.wait_until("disabled status arrives", |h| !h.latest_status().auto_enabled);

        h.signals.set(zoom_in_meeting());
        h.wait_until("meeting detection arrives", |h| !h.latest_status().meetings.is_empty());

        assert!(h.recorder.state().started.is_empty());
        h.shut_down();
    }

    #[test]
    fn records_a_detected_meeting_on_manual_instruction() {
        let h = harness();
        h.commands.send(Command::SetAutoEnabled(false)).unwrap();
        h.wait_until("disabled status arrives", |h| !h.latest_status().auto_enabled);
        h.signals.set(zoom_in_meeting());
        h.wait_until("meeting detection arrives", |h| !h.latest_status().meetings.is_empty());

        let key = h.latest_status().meetings[0].key.clone();
        h.commands
            .send(Command::StartManual(ManualTarget::Meeting(key.clone())))
            .unwrap();
        h.wait_until("recording status arrives", |h| {
            h.latest_status().is_recording_meeting(&key)
        });

        assert_eq!(h.recorder.state().started[0].audio_process_id, Some(100));
        h.shut_down();
    }

    #[test]
    fn finalizes_recording_then_acknowledges_on_quit_instruction() {
        let h = harness();
        h.commands.send(Command::StartManual(ManualTarget::Screen)).unwrap();
        h.wait_until("recording", |h| h.recorder.state().recording);
        let recorder = h.recorder.clone();

        let updates = h.shut_down();

        assert!(!recorder.state().recording);
        assert_eq!(recorder.state().stop_count, 1);
        assert!(matches!(updates.last(), Some(Update::Status(status)) if !status.is_recording()));
    }

    #[test]
    fn finalizes_recording_and_exits_when_the_instruction_sender_is_gone() {
        let h = harness();
        h.commands.send(Command::StartManual(ManualTarget::Screen)).unwrap();
        h.wait_until("recording", |h| h.recorder.state().recording);
        let Harness {
            commands,
            recorder,
            thread,
            ..
        } = h;

        drop(commands);
        thread.join().unwrap();

        assert!(!recorder.state().recording);
    }
}
