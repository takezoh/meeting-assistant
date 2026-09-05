//! Process lifecycle and package-identity collector (contract-process-package-identity).
//!
//! The collector polls a [`ProcessEnumerator`] and turns the difference between consecutive
//! snapshots into `ProcessStarted`, `PackageIdentityObserved` and `ProcessStopped` signals for the
//! target applications only. Its first signal is always `CollectorStarted`; a target process that
//! is already running at the first poll gets a `ProcessStarted` with `payload.restart_resync = true`
//! so the detector's `resync-no-autostart` rule has something to act on.

use crate::package_identity::{PackageIdentity, PackageIdentityProbe};
use crate::Clock;
use ma_core_types::id::TypedId;
use ma_core_types::SignalId;
use ma_signal::{Authority, Payload, Signal, SignalKind, SignalSource, Subject, SCHEMA_VERSION};
use std::collections::{BTreeMap, VecDeque};

/// The `source_id` every signal of this collector carries.
pub const SOURCE_ID: &str = "os.process";

/// One process as the OS reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub parent_pid: u32,
    /// Executable file name (no directory), as reported by the process snapshot.
    pub image_name: String,
}

/// Why a snapshot could not be taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnumerationError {
    /// The OS call failed with this code; the collector keeps its previous view.
    Os { code: u32 },
}

/// Enumerates the running processes. Live implementation: [`WindowsProcessEnumerator`]; the
/// portable tests script [`FakeProcessEnumerator`].
pub trait ProcessEnumerator {
    fn snapshot(&mut self) -> Result<Vec<ProcessRecord>, EnumerationError>;
}

/// The service identifiers the collector matches on. Both lists come from the `ma-adapter-*`
/// tables through the composition root; this crate never carries them as literals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetApplications {
    /// Executable image names, compared case-insensitively.
    pub image_names: Vec<String>,
    /// Package family names, compared exactly.
    pub package_family_names: Vec<String>,
}

impl TargetApplications {
    pub fn is_empty(&self) -> bool {
        self.image_names.is_empty() && self.package_family_names.is_empty()
    }
    fn matches_image(&self, image_name: &str) -> bool {
        self.image_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(image_name))
    }
    fn matches_package(&self, identity: &PackageIdentity) -> bool {
        match identity {
            PackageIdentity::Packaged(name) => self.package_family_names.iter().any(|n| n == name),
            _ => false,
        }
    }
}

/// Internal counters that distinguish what the closed envelope cannot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectorDiagnostics {
    /// The collector was constructed with no identifiers and therefore observes nothing.
    pub empty_target_set: bool,
    /// Snapshots that failed and left the previous view in place.
    pub enumeration_failures: u32,
    /// Package-identity probes that failed (reported as `None`, unlike a confirmed non-packaged process).
    pub package_query_failures: u32,
    /// Processes confirmed as classic non-packaged executables.
    pub not_packaged: u32,
    /// Polls performed so far (the first poll is the resync poll).
    pub polls: u32,
}

#[derive(Debug, Clone)]
struct Tracked {
    subject: Subject,
    tree_root_pid: u32,
}

/// The collector. Generic over its three seams so the same code runs on Windows and in the
/// portable tests.
pub struct ProcessPackageCollector<E, P, C> {
    enumerator: E,
    probe: P,
    clock: C,
    targets: TargetApplications,
    known: BTreeMap<u32, Tracked>,
    queue: VecDeque<Signal>,
    started: bool,
    diagnostics: CollectorDiagnostics,
}

impl<E: ProcessEnumerator, P: PackageIdentityProbe, C: Clock> ProcessPackageCollector<E, P, C> {
    pub fn new(enumerator: E, probe: P, clock: C, targets: TargetApplications) -> Self {
        let diagnostics = CollectorDiagnostics {
            empty_target_set: targets.is_empty(),
            ..CollectorDiagnostics::default()
        };
        Self {
            enumerator,
            probe,
            clock,
            targets,
            known: BTreeMap::new(),
            queue: VecDeque::new(),
            started: false,
            diagnostics,
        }
    }

    pub fn diagnostics(&self) -> &CollectorDiagnostics {
        &self.diagnostics
    }

    fn signal(&mut self, kind: SignalKind, subject: Subject, payload: Payload) -> Signal {
        Signal {
            signal_id: SignalId::new(),
            source_id: SOURCE_ID.to_string(),
            kind,
            subject,
            observed_at: self.clock.now(),
            payload,
            authority: Authority::Os,
            schema_version: SCHEMA_VERSION,
        }
    }

    /// Takes one snapshot and queues the difference to the previous view.
    ///
    /// Returns `false` when the snapshot failed; the queue is then left untouched.
    pub fn poll(&mut self) -> bool {
        let first_poll = self.diagnostics.polls == 0;
        self.diagnostics.polls += 1;
        let records = match self.enumerator.snapshot() {
            Ok(r) => r,
            Err(EnumerationError::Os { .. }) => {
                self.diagnostics.enumeration_failures += 1;
                return false;
            }
        };
        if self.targets.is_empty() {
            return true;
        }
        let by_pid: BTreeMap<u32, &ProcessRecord> = records.iter().map(|r| (r.pid, r)).collect();
        // Started processes, in pid order for deterministic fixtures.
        for record in &records {
            if self.known.contains_key(&record.pid) {
                continue;
            }
            let identity = self.probe.probe(record.pid);
            match &identity {
                PackageIdentity::QueryFailed { .. } => self.diagnostics.package_query_failures += 1,
                PackageIdentity::NotPackaged => self.diagnostics.not_packaged += 1,
                PackageIdentity::Packaged(_) => {}
            }
            if !(self.targets.matches_image(&record.image_name)
                || self.targets.matches_package(&identity))
            {
                continue;
            }
            let subject = Subject::Process {
                pid: record.pid,
                image_name: record.image_name.clone(),
                package_family_name: identity.family_name(),
            };
            let tree_root_pid = tree_root(&by_pid, record);
            let payload = Payload {
                restart_resync: first_poll,
                process_tree_root_pid: Some(tree_root_pid),
                ..Payload::default()
            };
            let started = self.signal(SignalKind::ProcessStarted, subject.clone(), payload);
            self.queue.push_back(started);
            let identity_payload = Payload {
                process_tree_root_pid: Some(tree_root_pid),
                ..Payload::default()
            };
            let observed = self.signal(
                SignalKind::PackageIdentityObserved,
                subject.clone(),
                identity_payload,
            );
            self.queue.push_back(observed);
            self.known.insert(
                record.pid,
                Tracked {
                    subject,
                    tree_root_pid,
                },
            );
        }
        // Stopped processes.
        let gone: Vec<u32> = self
            .known
            .keys()
            .copied()
            .filter(|pid| !by_pid.contains_key(pid))
            .collect();
        for pid in gone {
            let tracked = self.known.remove(&pid).expect("listed from known");
            let payload = Payload {
                process_tree_root_pid: Some(tracked.tree_root_pid),
                ..Payload::default()
            };
            let stopped = self.signal(SignalKind::ProcessStopped, tracked.subject, payload);
            self.queue.push_back(stopped);
        }
        true
    }
}

/// The topmost ancestor with the same image name, following parent pids inside the snapshot.
/// Browsers and Electron applications fork helper processes from one root; the root pid is what
/// tab and microphone facts are joined on.
fn tree_root(by_pid: &BTreeMap<u32, &ProcessRecord>, record: &ProcessRecord) -> u32 {
    let mut current = record;
    let mut hops = 0;
    while let Some(parent) = by_pid.get(&current.parent_pid) {
        if parent.pid == current.pid
            || !parent.image_name.eq_ignore_ascii_case(&record.image_name)
            || hops > 64
        {
            break;
        }
        current = parent;
        hops += 1;
    }
    current.pid
}

impl<E: ProcessEnumerator, P: PackageIdentityProbe, C: Clock> SignalSource
    for ProcessPackageCollector<E, P, C>
{
    fn source_id(&self) -> &str {
        SOURCE_ID
    }

    fn next_signal(&mut self) -> Option<Signal> {
        if !self.started {
            self.started = true;
            let started = self.signal(
                SignalKind::CollectorStarted,
                Subject::System,
                Payload::default(),
            );
            return Some(started);
        }
        if self.queue.is_empty() {
            self.poll();
        }
        self.queue.pop_front()
    }
}

/// Scripted snapshots: each call to `snapshot` returns the next scripted view, repeating the last
/// one once the script is exhausted.
#[derive(Debug, Default, Clone)]
pub struct FakeProcessEnumerator {
    views: VecDeque<Result<Vec<ProcessRecord>, EnumerationError>>,
    last: Option<Vec<ProcessRecord>>,
}

impl FakeProcessEnumerator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn then_view(mut self, records: Vec<ProcessRecord>) -> Self {
        self.views.push_back(Ok(records));
        self
    }
    pub fn then_failure(mut self, code: u32) -> Self {
        self.views.push_back(Err(EnumerationError::Os { code }));
        self
    }
}

impl ProcessEnumerator for FakeProcessEnumerator {
    fn snapshot(&mut self) -> Result<Vec<ProcessRecord>, EnumerationError> {
        match self.views.pop_front() {
            Some(Ok(view)) => {
                self.last = Some(view.clone());
                Ok(view)
            }
            Some(Err(e)) => Err(e),
            None => Ok(self.last.clone().unwrap_or_default()),
        }
    }
}

/// `CreateToolhelp32Snapshot` + `Process32FirstW`/`Process32NextW`.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsProcessEnumerator;

#[cfg(windows)]
impl ProcessEnumerator for WindowsProcessEnumerator {
    fn snapshot(&mut self) -> Result<Vec<ProcessRecord>, EnumerationError> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        // SAFETY: plain snapshot request; the handle is closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|e| {
            EnumerationError::Os {
                code: e.code().0 as u32,
            }
        })?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut out = Vec::new();
        // SAFETY: entry.dwSize is set as the API requires.
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
        while ok {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            out.push(ProcessRecord {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
                image_name: String::from_utf16_lossy(&entry.szExeFile[..end]),
            });
            // SAFETY: same entry buffer, advanced by the API.
            ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
        }
        // SAFETY: handle from CreateToolhelp32Snapshot above.
        let _ = unsafe { CloseHandle(snapshot) };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_identity::FakePackageIdentityProbe;
    use crate::SteppingClock;

    fn record(pid: u32, parent_pid: u32, image: &str) -> ProcessRecord {
        ProcessRecord {
            pid,
            parent_pid,
            image_name: image.to_string(),
        }
    }

    fn targets() -> TargetApplications {
        TargetApplications {
            image_names: vec!["example-meetings.exe".into(), "example-browser.exe".into()],
            package_family_names: vec!["ExamplePublisher.Meetings_8wekyb3d8bbwe".into()],
        }
    }

    fn clock() -> SteppingClock {
        SteppingClock::new(1_000_000, 1_000_000, 1_756_857_600_000)
    }

    fn drain<S: SignalSource>(source: &mut S, max: usize) -> Vec<Signal> {
        let mut out = Vec::new();
        while out.len() < max {
            match source.next_signal() {
                Some(s) => out.push(s),
                None => break,
            }
        }
        out
    }

    #[test]
    fn process_identity_from_fake_enumerator() {
        // The four target applications: one packaged, one classic, a browser tree, and an
        // unrelated process that must never be reported.
        let view = vec![
            record(4, 0, "system"),
            record(100, 4, "example-meetings.exe"),
            record(200, 4, "example-browser.exe"),
            record(201, 200, "example-browser.exe"),
            record(300, 4, "unrelated.exe"),
        ];
        let enumerator = FakeProcessEnumerator::new()
            .then_view(view.clone())
            .then_view(vec![
                record(4, 0, "system"),
                record(200, 4, "example-browser.exe"),
            ]);
        let probe = FakePackageIdentityProbe::default()
            .with(
                100,
                PackageIdentity::Packaged("ExamplePublisher.Meetings_8wekyb3d8bbwe".into()),
            )
            .with(200, PackageIdentity::NotPackaged)
            .with(201, PackageIdentity::NotPackaged)
            .with(300, PackageIdentity::NotPackaged);
        let mut collector = ProcessPackageCollector::new(enumerator, probe, clock(), targets());

        let first = collector.next_signal().expect("collector announces itself");
        assert_eq!(first.kind, SignalKind::CollectorStarted);
        assert_eq!(first.subject, Subject::System);
        assert_eq!(first.source_id, SOURCE_ID);
        assert_eq!(first.authority, Authority::Os);

        let batch = drain(&mut collector, 6);
        let kinds: Vec<(SignalKind, u32)> = batch
            .iter()
            .map(|s| match &s.subject {
                Subject::Process { pid, .. } => (s.kind, *pid),
                other => panic!("process subject expected, got {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                (SignalKind::ProcessStarted, 100),
                (SignalKind::PackageIdentityObserved, 100),
                (SignalKind::ProcessStarted, 200),
                (SignalKind::PackageIdentityObserved, 200),
                (SignalKind::ProcessStarted, 201),
                (SignalKind::PackageIdentityObserved, 201),
            ]
        );
        assert_eq!(
            batch[0].subject,
            Subject::Process {
                pid: 100,
                image_name: "example-meetings.exe".into(),
                package_family_name: Some("ExamplePublisher.Meetings_8wekyb3d8bbwe".into()),
            }
        );
        assert_eq!(
            batch[2].subject,
            Subject::Process {
                pid: 200,
                image_name: "example-browser.exe".into(),
                package_family_name: None,
            }
        );
        // The browser helper joins on its tree root; no other payload field is set.
        assert_eq!(batch[4].payload.process_tree_root_pid, Some(200));
        assert_eq!(batch[2].payload.process_tree_root_pid, Some(200));
        assert!(batch[1].payload.audible.is_none() && batch[1].payload.command.is_none());
        assert!(batch.iter().all(|s| s.schema_version == SCHEMA_VERSION));
        // Monotonic clock strictly increases across the batch.
        assert!(batch
            .windows(2)
            .all(|w| w[0].observed_at.monotonic_ns < w[1].observed_at.monotonic_ns));

        // Second view: the meetings app and the browser helper are gone.
        let stopped = drain(&mut collector, 2);
        let mut stopped_pids: Vec<u32> = stopped
            .iter()
            .map(|s| match &s.subject {
                Subject::Process { pid, .. } => *pid,
                _ => unreachable!(),
            })
            .collect();
        stopped_pids.sort();
        assert!(stopped.iter().all(|s| s.kind == SignalKind::ProcessStopped));
        assert_eq!(stopped_pids, vec![100, 201]);
        assert!(!stopped[0].payload.restart_resync);
    }

    #[test]
    fn restart_while_condition_true_sets_restart_resync() {
        let already_running = vec![record(100, 4, "example-meetings.exe")];
        let enumerator = FakeProcessEnumerator::new()
            .then_view(already_running)
            .then_view(vec![
                record(100, 4, "example-meetings.exe"),
                record(200, 4, "example-browser.exe"),
            ]);
        let probe = FakePackageIdentityProbe::default();
        let mut collector = ProcessPackageCollector::new(enumerator, probe, clock(), targets());
        assert_eq!(
            collector.next_signal().unwrap().kind,
            SignalKind::CollectorStarted
        );
        let first_started = collector.next_signal().unwrap();
        assert_eq!(first_started.kind, SignalKind::ProcessStarted);
        assert!(
            first_started.payload.restart_resync,
            "a process already running when the collector starts is a resync, not a fresh start"
        );
        let _identity = collector.next_signal().unwrap();
        let later_started = collector.next_signal().unwrap();
        assert_eq!(later_started.kind, SignalKind::ProcessStarted);
        assert!(
            !later_started.payload.restart_resync,
            "a process that starts after the first poll is a fresh start"
        );
    }

    #[test]
    fn query_failure_and_not_packaged_both_report_none_but_diagnostics_differ() {
        let view = vec![
            record(100, 4, "example-meetings.exe"),
            record(200, 4, "example-browser.exe"),
        ];
        let enumerator = FakeProcessEnumerator::new().then_view(view);
        let probe = FakePackageIdentityProbe::default()
            .with(100, PackageIdentity::QueryFailed { code: 5 })
            .with(200, PackageIdentity::NotPackaged);
        let mut collector = ProcessPackageCollector::new(enumerator, probe, clock(), targets());
        let _ = collector.next_signal();
        let batch = drain(&mut collector, 4);
        for s in &batch {
            match &s.subject {
                Subject::Process {
                    package_family_name,
                    ..
                } => assert_eq!(*package_family_name, None),
                _ => unreachable!(),
            }
        }
        assert_eq!(collector.diagnostics().package_query_failures, 1);
        assert_eq!(collector.diagnostics().not_packaged, 1);
    }

    #[test]
    fn empty_target_set_observes_nothing_and_says_so() {
        let enumerator =
            FakeProcessEnumerator::new().then_view(vec![record(100, 4, "example-meetings.exe")]);
        let mut collector = ProcessPackageCollector::new(
            enumerator,
            FakePackageIdentityProbe::default(),
            clock(),
            TargetApplications::default(),
        );
        assert!(collector.diagnostics().empty_target_set);
        assert_eq!(
            collector.next_signal().unwrap().kind,
            SignalKind::CollectorStarted
        );
        assert!(collector.next_signal().is_none());
    }

    #[test]
    fn enumeration_failure_keeps_the_previous_view() {
        let enumerator = FakeProcessEnumerator::new()
            .then_view(vec![record(100, 4, "example-meetings.exe")])
            .then_failure(5)
            .then_view(vec![]);
        let mut collector = ProcessPackageCollector::new(
            enumerator,
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let _ = collector.next_signal();
        let started = drain(&mut collector, 2);
        assert_eq!(started.len(), 2);
        // Failed poll: nothing emitted, failure counted, process still known.
        assert!(collector.next_signal().is_none());
        assert_eq!(collector.diagnostics().enumeration_failures, 1);
        // Next successful poll reports the stop.
        let stopped = collector.next_signal().unwrap();
        assert_eq!(stopped.kind, SignalKind::ProcessStopped);
    }
}
