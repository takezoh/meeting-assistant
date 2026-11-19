//! Fakes of the ports, for tests. They let the coordinator's behavior be verified without touching the real OS.
//! Included only when the `fakes` feature is enabled.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::apps::{MeetingApp, WindowRole};
use crate::ports::{Clock, CoordinatorPorts, LocalDateTime, OutputFolder, Pause};
use crate::recording::{Recorder, RecordingRequest};
use crate::signals::{
    AudioSession, BrowserLink, BrowserMeeting, ProcessEntry, SignalSnapshot, SignalSource, WindowEntry,
};

pub const MIC: &str = "mic-1";
pub const SPEAKER: &str = "speaker-1";

/// Location of the fake output folder.
pub const OUTPUT_ROOT: &str = "recordings";

/// The date and time the fake clock points to.
pub const FIXED_TIME: LocalDateTime = LocalDateTime {
    year: 2026,
    month: 11,
    day: 17,
    hour: 10,
    minute: 0,
    second: 0,
};

#[derive(Default)]
pub struct RecorderState {
    /// Instructions for recordings that started successfully.
    pub started: Vec<RecordingRequest>,
    /// Makes subsequent starts fail, in this order.
    pub start_failures: VecDeque<String>,
    pub stop_count: u32,
    pub recording: bool,
    pub failure: Option<String>,
    pub retargets: Vec<u64>,
    pub retarget_fails: bool,
    /// While `Some`, started recordings report a file with this extension, as a tab recorded as WebM does.
    pub written_extension: Option<String>,
}

/// A recorder that only logs the instructions it receives. Clones share state,
/// so tests can still inspect it after handing it over.
#[derive(Clone, Default)]
pub struct FakeRecorder(Arc<Mutex<RecorderState>>);

impl FakeRecorder {
    pub fn state(&self) -> MutexGuard<'_, RecorderState> {
        self.0.lock().unwrap()
    }

    /// Causes a failure during recording.
    pub fn fail(&self, error: &str) {
        let mut state = self.state();
        state.recording = false;
        state.failure = Some(error.to_owned());
    }
}

impl Recorder for FakeRecorder {
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<PathBuf> {
        let mut state = self.state();
        if let Some(error) = state.start_failures.pop_front() {
            anyhow::bail!(error);
        }
        state.started.push(request.clone());
        state.recording = true;
        Ok(match &state.written_extension {
            Some(extension) => request.output_path.with_extension(extension),
            None => request.output_path.clone(),
        })
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        let mut state = self.state();
        state.stop_count += 1;
        state.recording = false;
        Ok(())
    }

    fn retarget(&mut self, window: u64) -> anyhow::Result<()> {
        let mut state = self.state();
        anyhow::ensure!(!state.retarget_fails, "window is gone");
        state.retargets.push(window);
        Ok(())
    }

    fn take_failure(&mut self) -> Option<String> {
        self.state().failure.take()
    }
}

#[derive(Default)]
struct SignalScript {
    next: SignalSnapshot,
    failure: Option<String>,
    /// The OS reads made, in order, until taken.
    reads: Vec<&'static str>,
}

/// A signal source that returns the signals the test sets.
#[derive(Clone, Default)]
pub struct ScriptedSignals(Arc<Mutex<SignalScript>>);

impl ScriptedSignals {
    pub fn set(&self, snapshot: SignalSnapshot) {
        self.0.lock().unwrap().next = snapshot;
    }

    /// While `Some`, makes signal acquisition fail.
    pub fn set_failure(&self, failure: Option<&str>) {
        self.0.lock().unwrap().failure = failure.map(str::to_owned);
    }

    /// The OS reads made since the last call: `capture_sessions`, `render_sessions`, `processes`, `windows`.
    pub fn take_reads(&self) -> Vec<&'static str> {
        std::mem::take(&mut self.0.lock().unwrap().reads)
    }

    fn read<T>(&self, name: &'static str, part: impl Fn(&SignalSnapshot) -> T) -> anyhow::Result<T> {
        let mut script = self.0.lock().unwrap();
        script.reads.push(name);
        match &script.failure {
            Some(error) => anyhow::bail!(error.clone()),
            None => Ok(part(&script.next)),
        }
    }
}

impl SignalSource for ScriptedSignals {
    fn capture_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.read("capture_sessions", |s| s.capture_sessions.clone())
    }

    fn render_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.read("render_sessions", |s| s.render_sessions.clone())
    }

    fn processes(&mut self) -> anyhow::Result<Vec<ProcessEntry>> {
        self.read("processes", |s| s.processes.clone())
    }

    fn windows(&mut self) -> anyhow::Result<Vec<WindowEntry>> {
        self.read("windows", |s| s.windows.clone())
    }

    fn browser(&mut self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        let script = self.0.lock().unwrap();
        (script.next.browser_links.clone(), script.next.browser_meetings.clone())
    }
}

/// A clock that gives the same date and time whenever asked.
pub struct FixedClock(pub LocalDateTime);

impl Clock for FixedClock {
    fn now(&self) -> LocalDateTime {
        self.0
    }
}

#[derive(Default)]
pub struct FolderState {
    pub prepared: u32,
    /// While `Some`, makes preparing the folder fail.
    pub prepare_failure: Option<String>,
    /// Names of files that already exist.
    pub files: Vec<String>,
}

/// An output folder that does not touch the file system. Recordings from the fake recorder count as existing.
#[derive(Clone)]
pub struct FakeOutputFolder {
    state: Arc<Mutex<FolderState>>,
    recorder: FakeRecorder,
}

impl FakeOutputFolder {
    pub fn new(recorder: &FakeRecorder) -> Self {
        Self {
            state: Arc::default(),
            recorder: recorder.clone(),
        }
    }

    pub fn state(&self) -> MutexGuard<'_, FolderState> {
        self.state.lock().unwrap()
    }
}

impl OutputFolder for FakeOutputFolder {
    fn prepare(&mut self) -> anyhow::Result<()> {
        let mut state = self.state();
        if let Some(error) = &state.prepare_failure {
            anyhow::bail!(error.clone());
        }
        state.prepared += 1;
        Ok(())
    }

    fn contains(&self, file_name: &str) -> bool {
        let recorded = self
            .recorder
            .state()
            .started
            .iter()
            .any(|request| request.output_path == self.path_of(file_name));
        recorded || self.state().files.iter().any(|name| name == file_name)
    }

    fn path_of(&self, file_name: &str) -> PathBuf {
        PathBuf::from(OUTPUT_ROOT).join(file_name)
    }
}

/// Does not wait; records the requested wait times instead.
#[derive(Clone, Default)]
pub struct RecordedPauses(Arc<Mutex<Vec<Duration>>>);

impl RecordedPauses {
    pub fn taken(&self) -> Vec<Duration> {
        self.0.lock().unwrap().clone()
    }
}

impl Pause for RecordedPauses {
    fn pause(&mut self, duration: Duration) {
        self.0.lock().unwrap().push(duration);
    }
}

/// A set of ports made up only of fakes. Keep clones of the output folder and wait log to inspect them from tests.
pub fn fake_ports(recorder: &FakeRecorder) -> CoordinatorPorts {
    fake_ports_with(FakeOutputFolder::new(recorder), RecordedPauses::default())
}

pub fn fake_ports_with(output: FakeOutputFolder, pauses: RecordedPauses) -> CoordinatorPorts {
    CoordinatorPorts {
        clock: Box::new(FixedClock(FIXED_TIME)),
        output: Box::new(output),
        pause: Box::new(pauses),
    }
}

/// Location of the file with that name in the fake output folder.
pub fn output_path(file_name: &str) -> PathBuf {
    PathBuf::from(OUTPUT_ROOT).join(file_name)
}

// ---- Helpers for building signal snapshots

pub fn process(process_id: u32, parent_process_id: u32, name: &str) -> ProcessEntry {
    ProcessEntry {
        process_id,
        parent_process_id,
        name: name.to_owned(),
    }
}

pub fn window(process_id: u32, title: &str, display: &str, area: u64) -> WindowEntry {
    WindowEntry {
        id: window_id(title),
        process_id,
        title: title.to_owned(),
        display: Some(display.to_owned()),
        area,
    }
}

/// In tests, a window's identifier is derived from its title (so a window can be addressed by title).
pub fn window_id(title: &str) -> u64 {
    title.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

fn sessions(pids: &[u32], device: &str) -> Vec<AudioSession> {
    pids.iter()
        .map(|&process_id| AudioSession {
            process_id,
            device_id: device.to_owned(),
        })
        .collect()
}

/// Builds a snapshot of processes parented under explorer(1), plus optional sessions and windows.
pub fn snapshot(
    processes: &[ProcessEntry],
    mic_pids: &[u32],
    render_pids: &[u32],
    windows: &[WindowEntry],
) -> SignalSnapshot {
    let mut all = vec![process(1, 0, "explorer.exe")];
    all.extend_from_slice(processes);
    SignalSnapshot {
        capture_sessions: sessions(mic_pids, MIC),
        render_sessions: sessions(render_pids, SPEAKER),
        processes: all,
        windows: windows.to_vec(),
        ..Default::default()
    }
}

pub fn zoom_in_meeting() -> SignalSnapshot {
    snapshot(&[process(100, 1, "Zoom.exe")], &[100], &[], &[])
}

/// A meeting app adapter for the app run as `process_name`: a window whose title starts with "main:" is its main
/// window, one whose title starts with "meeting:" a meeting's, and nothing is told of the others.
#[derive(Debug)]
pub struct FakeMeetingApp {
    pub process_name: &'static str,
}

impl MeetingApp for FakeMeetingApp {
    fn handles(&self, process_name: &str) -> bool {
        process_name == self.process_name
    }

    fn window_roles(&self, titles: &[&str]) -> Vec<WindowRole> {
        titles
            .iter()
            .map(|title| {
                if title.starts_with("main:") {
                    WindowRole::Main
                } else if title.starts_with("meeting:") {
                    WindowRole::Meeting
                } else {
                    WindowRole::Unknown
                }
            })
            .collect()
    }
}
