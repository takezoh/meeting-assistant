//! `ma-diag`: the Phase 1 diagnostic harness (contract-diagnostic-session-harness, NFR-105).
//!
//! Without a subcommand nothing starts and nothing is written. `list` prints what the adapter
//! tables declare. `record` is the only command that starts collectors and opens capture sources;
//! it appends every observed signal to the session timeline before reading the next one, and
//! writes the decisions sidecar when the session stops. `label` attaches a `was_meeting` range to a
//! timeline's sidecar (FR-109). `replay` runs the offline detector path over a recorded timeline.
//! `measure-leak` computes the echo return loss over two recorded tracks.

// The harness module is shared by the binary and its tests; the parts of it the binary does not
// reach on a given host (the live path on non-Windows, the fakes and accessors on Windows) are
// exercised by the tests, so dead-code lints are not meaningful for it.
#[allow(dead_code, unused_imports)]
#[path = "../diagnostic/mod.rs"]
mod diagnostic;

use diagnostic::{AdapterTables, Command};
#[cfg(any(windows, test))]
use ma_capture::wasapi::{EndpointChoice, EndpointSelection};
#[cfg(any(windows, test))]
use ma_capture::{CaptureSource, SourceEvent};
#[cfg(any(windows, test))]
use ma_core_types::timeline::TrackOrigin;
#[cfg(any(windows, test))]
use std::collections::VecDeque;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(any(windows, test))]
use std::sync::mpsc::{SyncSender, TrySendError};

const EXIT_USAGE: u8 = 2;
#[cfg(not(windows))]
const EXIT_UNSUPPORTED_PLATFORM: u8 = 4;

#[cfg(any(windows, test))]
#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
struct EndpointSelectionEvidence {
    opened_endpoint: String,
    selection_history: Vec<String>,
    coalesced_hints: u32,
    failed_switches: u32,
}

#[cfg(any(windows, test))]
fn endpoint_choice_label(choice: &EndpointChoice) -> String {
    match choice {
        EndpointChoice::Supplied(id) => id.clone(),
        EndpointChoice::SystemDefault => "system-default".to_string(),
    }
}

#[cfg(any(windows, test))]
fn endpoint_selection_evidence(selection: &EndpointSelection) -> EndpointSelectionEvidence {
    EndpointSelectionEvidence {
        opened_endpoint: endpoint_choice_label(&selection.opened),
        selection_history: selection
            .history
            .iter()
            .map(endpoint_choice_label)
            .collect(),
        coalesced_hints: selection.coalesced_hints,
        failed_switches: selection.failed_switches,
    }
}

#[cfg(any(windows, test))]
fn write_endpoint_selection(
    timeline: &std::path::Path,
    selection: &EndpointSelection,
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    let path = timeline.with_extension("endpoint-selection.json");
    let bytes = serde_json::to_vec_pretty(&endpoint_selection_evidence(selection))?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.flush()?;
    file.sync_all()?;
    Ok(path)
}

/// Events crossing from the source-service thread to the durability thread. Filesystem work is
/// deliberately absent from the producer side of this boundary.
#[derive(Debug)]
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
enum CaptureWriteEvent {
    Samples(Vec<i16>),
    Gap(u64),
    FormatChanged(TrackOrigin),
    Ended,
}

/// Non-blocking, bounded handoff from capture servicing to durable writing. When the writer is
/// behind, sample payloads are replaced by an ordered explicit gap instead of blocking WASAPI.
#[cfg(any(windows, test))]
struct CaptureHandoff {
    tx: SyncSender<CaptureWriteEvent>,
    pending: VecDeque<CaptureWriteEvent>,
    disconnected: bool,
}

#[cfg(any(windows, test))]
impl CaptureHandoff {
    fn new(tx: SyncSender<CaptureWriteEvent>) -> Self {
        Self {
            tx,
            pending: VecDeque::new(),
            disconnected: false,
        }
    }

    fn retain_unsent(&mut self, event: CaptureWriteEvent) {
        match event {
            CaptureWriteEvent::Samples(samples) => self.retain_gap(samples.len() as u64),
            other => self.pending.push_back(other),
        }
    }

    fn retain_gap(&mut self, samples: u64) {
        if samples == 0 {
            return;
        }
        match self.pending.back_mut() {
            Some(CaptureWriteEvent::Gap(pending)) => *pending += samples,
            _ => self.pending.push_back(CaptureWriteEvent::Gap(samples)),
        }
    }

    fn flush_pending(&mut self) {
        while let Some(event) = self.pending.pop_front() {
            match self.tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(event)) => {
                    self.pending.push_front(event);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.disconnected = true;
                    self.pending.clear();
                    break;
                }
            }
        }
    }

    fn forward(&mut self, event: SourceEvent) -> bool {
        self.flush_pending();
        let event = match event {
            SourceEvent::Samples(samples) => CaptureWriteEvent::Samples(samples),
            SourceEvent::FormatChanged(origin) => CaptureWriteEvent::FormatChanged(origin),
            SourceEvent::Ended => CaptureWriteEvent::Ended,
        };
        if self.disconnected || !self.pending.is_empty() {
            self.retain_unsent(event);
            return !self.disconnected;
        }
        match self.tx.try_send(event) {
            Ok(()) => true,
            Err(TrySendError::Full(event)) => {
                self.retain_unsent(event);
                true
            }
            Err(TrySendError::Disconnected(_)) => {
                self.disconnected = true;
                false
            }
        }
    }

    fn gap(&mut self, samples: u64) -> bool {
        self.flush_pending();
        if self.disconnected || !self.pending.is_empty() {
            self.retain_gap(samples);
            return !self.disconnected;
        }
        match self.tx.try_send(CaptureWriteEvent::Gap(samples)) {
            Ok(()) => true,
            Err(TrySendError::Full(CaptureWriteEvent::Gap(samples))) => {
                self.retain_gap(samples);
                true
            }
            Err(TrySendError::Disconnected(_)) => {
                self.disconnected = true;
                false
            }
            Err(TrySendError::Full(_)) => unreachable!("gap send retains a gap"),
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn finish(mut self) {
        for event in self.pending.drain(..) {
            if self.tx.send(event).is_err() {
                return;
            }
        }
        let _ = self.tx.send(CaptureWriteEvent::Ended);
    }
}

#[cfg(any(windows, test))]
fn service_capture_once(
    source: &mut dyn CaptureSource,
    handoff: &mut CaptureHandoff,
    discontinuity_gap_samples: u64,
) -> bool {
    let event = source.next();
    let ended = matches!(event, SourceEvent::Ended);
    if !handoff.forward(event) {
        return false;
    }
    let discontinuities = source.take_discontinuities();
    if discontinuities != 0 && !handoff.gap(discontinuities as u64 * discontinuity_gap_samples) {
        return false;
    }
    !ended
}

#[cfg(any(windows, test))]
fn persist_batch<T, E>(
    items: impl IntoIterator<Item = T>,
    mut persist: impl FnMut(&T) -> Result<(), E>,
) -> Result<(), E> {
    for item in items {
        persist(&item)?;
    }
    Ok(())
}

/// How many times a live session was started in this process. `execute` for every command other
/// than `record` must leave it untouched (NFR-105), and the tests assert exactly that.
pub static RECORD_STARTS: AtomicUsize = AtomicUsize::new(0);

fn usage() -> u8 {
    eprintln!(
        "usage:\n  ma-diag list\n  ma-diag record --artifact-root DIR [--extension-dir DIR] [--max-rounds N]   (needs MA_EXTENSION_ID and, on Windows, the current user's SID in MA_OWNER_SID)\n  ma-diag label --timeline FILE --from-ns N --to-ns N --was-meeting true|false [--note TEXT]\n  ma-diag replay --timeline FILE [--synthetic-tables]\n  ma-diag measure-leak --loopback-track DIR --mic-track DIR --application ID --alignment-uncertainty-ms N [--out FILE]\n\nNothing starts without a subcommand; only `record` starts collectors or opens capture sources."
    );
    EXIT_USAGE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(execute(&args))
}

/// Dispatches one invocation and returns the process exit code. Only `Command::Record` starts a
/// collector, opens a capture source or writes under the artifact root.
pub fn execute(args: &[String]) -> u8 {
    let command = match Command::parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return usage();
        }
    };
    let mut starter = LiveCaptureStarter;
    execute_command(command, &mut starter)
}

trait CaptureStarter {
    fn start(
        &mut self,
        artifact_root: &std::path::Path,
        extension_dir: Option<&std::path::Path>,
        max_rounds: Option<u64>,
    ) -> u8;
}

struct LiveCaptureStarter;

impl CaptureStarter for LiveCaptureStarter {
    fn start(
        &mut self,
        artifact_root: &std::path::Path,
        extension_dir: Option<&std::path::Path>,
        max_rounds: Option<u64>,
    ) -> u8 {
        RECORD_STARTS.fetch_add(1, Ordering::SeqCst);
        record::run(artifact_root, extension_dir, max_rounds)
    }
}

fn execute_command(command: Command, starter: &mut dyn CaptureStarter) -> u8 {
    match command {
        Command::Usage => usage(),
        Command::List => {
            let tables = AdapterTables::load();
            let targets = tables.target_applications();
            println!("adapters: {}", tables.ids().join(", "));
            println!("process images: {}", targets.image_names.join(", "));
            println!(
                "package family names: {}",
                targets.package_family_names.join(", ")
            );
            println!("meeting hosts: {}", tables.meeting_hosts().join(", "));
            0
        }
        Command::Label {
            timeline,
            from_ns,
            to_ns,
            was_meeting,
            note,
        } => match diagnostic::label_timeline(&timeline, from_ns, to_ns, was_meeting, &note) {
            Ok(path) => {
                println!("labels written to {}", path.display());
                0
            }
            Err(e) => {
                eprintln!("label failed: {e}");
                1
            }
        },
        Command::Replay {
            timeline,
            synthetic_tables,
        } => {
            let tables = if synthetic_tables {
                AdapterTables::synthetic_fixture_tables()
            } else {
                AdapterTables::load()
            };
            match diagnostic::replay(&timeline, &tables) {
                Ok(output) => {
                    println!("{}", output.to_canonical_json());
                    0
                }
                Err(e) => {
                    eprintln!("replay failed: {e}");
                    1
                }
            }
        }
        Command::MeasureLeak {
            loopback_track,
            mic_track,
            application,
            alignment_uncertainty_ms,
            out,
        } => match diagnostic::measure_leak(
            &loopback_track,
            &mic_track,
            &application,
            alignment_uncertainty_ms,
        ) {
            Ok(record) => {
                let text = serde_json::to_string_pretty(&record).expect("record serializes");
                match out {
                    Some(path) => {
                        if let Err(e) = std::fs::write(&path, format!("{text}\n")) {
                            eprintln!("cannot write {}: {e}", path.display());
                            return 1;
                        }
                        println!("leak record written to {}", path.display());
                    }
                    None => println!("{text}"),
                }
                0
            }
            Err(e) => {
                eprintln!("measure-leak failed: {e}");
                1
            }
        },
        Command::Record {
            artifact_root,
            extension_dir,
            max_rounds,
        } => starter.start(&artifact_root, extension_dir.as_deref(), max_rounds),
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use ma_capture::SyntheticSource;
    use std::path::Path;
    use std::sync::mpsc;

    #[derive(Default)]
    struct SpyStarter(usize);
    impl CaptureStarter for SpyStarter {
        fn start(&mut self, _: &Path, _: Option<&Path>, _: Option<u64>) -> u8 {
            self.0 += 1;
            0
        }
    }

    #[test]
    fn every_non_record_command_has_zero_capture_starts() {
        let missing = std::path::PathBuf::from("definitely-missing-phase1-fixture");
        let commands = vec![
            Command::Usage,
            Command::List,
            Command::Label {
                timeline: missing.clone(),
                from_ns: 0,
                to_ns: 1,
                was_meeting: false,
                note: String::new(),
            },
            Command::Replay {
                timeline: missing.clone(),
                synthetic_tables: false,
            },
            Command::MeasureLeak {
                loopback_track: missing.clone(),
                mic_track: missing,
                application: "test".into(),
                alignment_uncertainty_ms: 1,
                out: None,
            },
        ];
        let mut spy = SpyStarter::default();
        for command in commands {
            let _ = execute_command(command, &mut spy);
        }
        assert_eq!(spy.0, 0);
        let _ = execute_command(
            Command::Record {
                artifact_root: std::path::PathBuf::from("unused"),
                extension_dir: None,
                max_rounds: Some(0),
            },
            &mut spy,
        );
        assert_eq!(
            spy.0, 1,
            "only an explicit valid Record dispatch reaches capture startup"
        );
    }

    #[test]
    fn stalled_writer_does_not_stop_source_reads_and_becomes_a_gap() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut handoff = CaptureHandoff::new(tx);
        let mut source = SyntheticSource::new(16_000, 800, 160);

        for _ in 0..5 {
            assert!(service_capture_once(&mut source, &mut handoff, 3_200));
        }
        assert_eq!(
            source.produced(),
            800,
            "capture reads continue while the durability queue is full"
        );

        assert!(matches!(
            rx.recv().unwrap(),
            CaptureWriteEvent::Samples(samples) if samples.len() == 160
        ));
        handoff.flush_pending();
        assert!(matches!(rx.recv().unwrap(), CaptureWriteEvent::Gap(640)));
    }

    #[test]
    fn persistence_failure_stops_before_the_next_item() {
        let mut attempted = Vec::new();
        let result = persist_batch([1, 2, 3], |value| {
            attempted.push(*value);
            if *value == 2 {
                Err("disk")
            } else {
                Ok(())
            }
        });

        assert_eq!(result, Err("disk"));
        assert_eq!(attempted, vec![1, 2]);
    }

    #[test]
    fn endpoint_selection_sidecar_contains_concrete_ordered_history() {
        let dir = tempfile::tempdir().unwrap();
        let timeline = dir.path().join("session.jsonl");
        let selection = EndpointSelection {
            opened: EndpointChoice::Supplied("endpoint-b".into()),
            history: vec![
                EndpointChoice::SystemDefault,
                EndpointChoice::Supplied("endpoint-a".into()),
                EndpointChoice::Supplied("endpoint-b".into()),
            ],
            coalesced_hints: 2,
            failed_switches: 1,
        };

        let path = write_endpoint_selection(&timeline, &selection).unwrap();
        let persisted: EndpointSelectionEvidence =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        assert_eq!(path, dir.path().join("session.endpoint-selection.json"));
        assert_eq!(persisted.opened_endpoint, "endpoint-b");
        assert_eq!(
            persisted.selection_history,
            ["system-default", "endpoint-a", "endpoint-b"]
        );
        assert_eq!(persisted.coalesced_hints, 2);
        assert_eq!(persisted.failed_switches, 1);
    }
}

#[cfg(not(windows))]
mod record {
    use super::EXIT_UNSUPPORTED_PLATFORM;
    use std::path::Path;

    /// The live collectors and capture sources are Windows-only; on any other host `record`
    /// refuses explicitly instead of running with fakes and pretending to observe a machine.
    pub fn run(
        _artifact_root: &Path,
        _extension_dir: Option<&Path>,
        _max_rounds: Option<u64>,
    ) -> u8 {
        eprintln!(
            "record needs a Windows host: the live collectors and WASAPI sources are Windows-only (this host is {}). Use `replay` for recorded timelines.",
            std::env::consts::OS
        );
        EXIT_UNSUPPORTED_PLATFORM
    }
}

#[cfg(windows)]
mod record {
    use super::diagnostic::{
        provision_extension, timeline_header, AdapterTables, DiagnosticSession, LoopbackListener,
        WindowsPeerResolver,
    };
    use super::{
        persist_batch, service_capture_once, write_endpoint_selection, CaptureHandoff,
        CaptureWriteEvent,
    };
    use ma_capture::wasapi::{
        origin_clock_from, LoopbackTarget, MicEndpointSource, WasapiSource,
        WindowsActivationBackend,
    };
    use ma_capture::{CaptureSource, ChunkWriter, RealFs, SourceEvent, SAMPLE_RATE};
    use ma_core_types::id::TypedId;
    use ma_core_types::TrackId;
    use ma_ext_channel::auth::WindowsAclApplier;
    use ma_ext_channel::{EndpointDescriptor, Server, ServerConfig, SystemClock as ChannelClock};
    use ma_signal::SignalSource;
    use ma_signals_windows::audio_session::WindowsSessionManager;
    use ma_signals_windows::mic_use::WindowsConsentStore;
    use ma_signals_windows::package_identity::WindowsPackageIdentityProbe;
    use ma_signals_windows::process::WindowsProcessEnumerator;
    use ma_signals_windows::{
        AudioSessionMicCollector, ProcessEnumerator, ProcessPackageCollector, SystemClock,
    };
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    /// Samples assumed lost per device-reported discontinuity: the shared-mode buffer the sources
    /// are opened with (200 ms), in the writer's sample rate. A lower bound, recorded as a gap.
    const DISCONTINUITY_GAP_SAMPLES: u64 = SAMPLE_RATE as u64 / 5;
    /// Bounded independently of the writer's internal 60-second sample queue. Overflow at this
    /// boundary is converted to an explicit gap by CaptureHandoff.
    const CAPTURE_EVENT_CAPACITY: usize = 256;

    enum LiveTrackSource {
        Loopback(WasapiSource<WindowsActivationBackend>),
        Microphone(MicEndpointSource<WindowsActivationBackend>),
    }

    impl LiveTrackSource {
        fn update_endpoint_hint(&mut self, endpoint: &str) {
            if let Self::Microphone(source) = self {
                source.update_hint(Some(endpoint));
            }
        }

        fn endpoint_selection(&self) -> Option<&ma_capture::wasapi::EndpointSelection> {
            match self {
                Self::Microphone(source) => Some(source.selection()),
                Self::Loopback(_) => None,
            }
        }
    }

    impl CaptureSource for LiveTrackSource {
        fn origin(&self) -> ma_core_types::timeline::TrackOrigin {
            match self {
                Self::Loopback(source) => source.origin(),
                Self::Microphone(source) => source.origin(),
            }
        }

        fn next(&mut self) -> SourceEvent {
            match self {
                Self::Loopback(source) => source.next(),
                Self::Microphone(source) => source.next(),
            }
        }

        fn take_discontinuities(&mut self) -> u32 {
            match self {
                Self::Loopback(source) => source.take_discontinuities(),
                Self::Microphone(source) => source.take_discontinuities(),
            }
        }
    }

    fn write_track_events(
        rx: mpsc::Receiver<CaptureWriteEvent>,
        tracks_root: PathBuf,
        role: &'static str,
        origin: ma_core_types::timeline::TrackOrigin,
    ) -> Result<(), String> {
        let mut fs = RealFs;
        let mut writer = ChunkWriter::open(&tracks_root.join(role), TrackId::new(), role, origin)
            .map_err(|error| format!("cannot open {role} track: {error:?}"))?;
        while let Ok(event) = rx.recv() {
            match event {
                CaptureWriteEvent::Samples(samples) => {
                    writer.push(&samples);
                    if let Err(error) = writer.drain(&mut fs) {
                        // The writer retains the unwritten samples. Continue consuming the
                        // bounded handoff so ChunkWriter can attribute any later overflow to an
                        // explicit disk-backpressure/disk-full gap and retry on the next event.
                        eprintln!("{role}: chunk write deferred: {error:?}");
                    }
                }
                CaptureWriteEvent::Gap(samples) => writer.record_capture_gap(samples),
                CaptureWriteEvent::FormatChanged(origin) => {
                    writer = writer
                        .open_successor(&mut fs, &tracks_root, TrackId::new(), origin, wall_ms())
                        .map_err(|error| format!("{role}: successor track failed: {error:?}"))?;
                }
                CaptureWriteEvent::Ended => {
                    return writer
                        .finish(&mut fs)
                        .map_err(|error| format!("{role}: final chunk failed: {error:?}"));
                }
            }
        }
        writer
            .finish(&mut fs)
            .map_err(|error| format!("{role}: final chunk failed: {error:?}"))
    }

    /// Owns a capture thread. Each thread creates and services its COM/WASAPI objects itself, so
    /// HTTP parsing, timeline fsync and collector polling can never starve the 200 ms audio buffer.
    struct TrackWorker {
        stop: Arc<AtomicBool>,
        failed: Arc<AtomicBool>,
        endpoint: Option<mpsc::Sender<String>>,
        selection: Option<Arc<Mutex<Option<ma_capture::wasapi::EndpointSelection>>>>,
        join: Option<JoinHandle<()>>,
    }

    impl TrackWorker {
        fn loopback(pid: u32, tracks_root: PathBuf, origin: Instant) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let failed = Arc::new(AtomicBool::new(false));
            let thread_stop = stop.clone();
            let thread_failed = failed.clone();
            let join = std::thread::spawn(move || {
                let source = match WasapiSource::open_process_loopback(
                    WindowsActivationBackend::new(),
                    LoopbackTarget {
                        pid,
                        include_process_tree: true,
                    },
                    origin_clock_from(origin),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        eprintln!("loopback activation failed: {error:?}");
                        thread_failed.store(true, AtomicOrdering::Release);
                        return;
                    }
                };
                eprintln!("loopback activation: {:?}", source.outcome());
                let initial_origin = source.origin();
                let (write_tx, write_rx) = mpsc::sync_channel(CAPTURE_EVENT_CAPACITY);
                let writer_failed = thread_failed.clone();
                let writer_join = std::thread::spawn(move || {
                    if let Err(error) =
                        write_track_events(write_rx, tracks_root, "loopback", initial_origin)
                    {
                        eprintln!("{error}");
                        writer_failed.store(true, AtomicOrdering::Release);
                    }
                });
                let mut source = LiveTrackSource::Loopback(source);
                let mut handoff = CaptureHandoff::new(write_tx);
                while !thread_stop.load(AtomicOrdering::Acquire)
                    && !thread_failed.load(AtomicOrdering::Acquire)
                    && service_capture_once(&mut source, &mut handoff, DISCONTINUITY_GAP_SAMPLES)
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                handoff.finish();
                if writer_join.join().is_err() {
                    thread_failed.store(true, AtomicOrdering::Release);
                }
            });
            Self {
                stop,
                failed,
                endpoint: None,
                selection: None,
                join: Some(join),
            }
        }

        fn microphone(endpoint: String, tracks_root: PathBuf, origin: Instant) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let failed = Arc::new(AtomicBool::new(false));
            let thread_stop = stop.clone();
            let thread_failed = failed.clone();
            let (endpoint_tx, endpoint_rx) = mpsc::channel::<String>();
            let selection = Arc::new(Mutex::new(None));
            let thread_selection = selection.clone();
            let join = std::thread::spawn(move || {
                let source = match MicEndpointSource::open(
                    WindowsActivationBackend::new(),
                    Some(&endpoint),
                    origin_clock_from(origin),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        eprintln!("microphone open failed: {error:?}");
                        thread_failed.store(true, AtomicOrdering::Release);
                        return;
                    }
                };
                let initial_origin = source.origin();
                let (write_tx, write_rx) = mpsc::sync_channel(CAPTURE_EVENT_CAPACITY);
                let writer_failed = thread_failed.clone();
                let writer_join = std::thread::spawn(move || {
                    if let Err(error) =
                        write_track_events(write_rx, tracks_root, "mic", initial_origin)
                    {
                        eprintln!("{error}");
                        writer_failed.store(true, AtomicOrdering::Release);
                    }
                });
                let mut source = LiveTrackSource::Microphone(source);
                if let (Some(current), Ok(mut snapshot)) =
                    (source.endpoint_selection(), thread_selection.lock())
                {
                    *snapshot = Some(current.clone());
                }
                let mut handoff = CaptureHandoff::new(write_tx);
                while !thread_stop.load(AtomicOrdering::Acquire)
                    && !thread_failed.load(AtomicOrdering::Acquire)
                {
                    while let Ok(next) = endpoint_rx.try_recv() {
                        source.update_endpoint_hint(&next);
                    }
                    let continues =
                        service_capture_once(&mut source, &mut handoff, DISCONTINUITY_GAP_SAMPLES);
                    if let (Some(current), Ok(mut snapshot)) =
                        (source.endpoint_selection(), thread_selection.lock())
                    {
                        *snapshot = Some(current.clone());
                    }
                    if !continues {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                handoff.finish();
                if writer_join.join().is_err() {
                    thread_failed.store(true, AtomicOrdering::Release);
                }
            });
            Self {
                stop,
                failed,
                endpoint: Some(endpoint_tx),
                selection: Some(selection),
                join: Some(join),
            }
        }

        fn update_endpoint(&self, endpoint: &str) {
            if let Some(tx) = &self.endpoint {
                if tx.send(endpoint.to_string()).is_err() {
                    self.failed.store(true, AtomicOrdering::Release);
                }
            }
        }

        fn is_failed(&self) -> bool {
            self.failed.load(AtomicOrdering::Acquire)
        }

        fn finish(mut self) -> (bool, Option<ma_capture::wasapi::EndpointSelection>) {
            self.stop.store(true, AtomicOrdering::Release);
            if let Some(join) = self.join.take() {
                if join.join().is_err() {
                    self.failed.store(true, AtomicOrdering::Release);
                }
            }
            let success = !self.is_failed();
            let selection = self
                .selection
                .as_ref()
                .and_then(|snapshot| snapshot.lock().ok()?.clone());
            (success, selection)
        }
    }

    pub fn run(artifact_root: &Path, extension_dir: Option<&Path>, max_rounds: Option<u64>) -> u8 {
        // The pinned extension id and the owner SID are inputs, not defaults: without them every
        // extension report would be a silent 403 and the token files would get a bogus owner.
        let pinned = match std::env::var("MA_EXTENSION_ID") {
            Ok(v) if v.len() == 32 && v.chars().all(|c| c.is_ascii_lowercase()) => v,
            _ => {
                eprintln!("record needs MA_EXTENSION_ID (the 32-character id Chrome assigned to the unpacked extension)");
                return 2;
            }
        };
        let owner_sid = match std::env::var("MA_OWNER_SID") {
            Ok(v) if v.starts_with("S-1-") => v,
            _ => {
                eprintln!(
                    "record needs MA_OWNER_SID (the current user's SID, e.g. from `whoami /user`)"
                );
                return 2;
            }
        };
        let tables = AdapterTables::load();
        let targets = tables.target_applications();
        // One monotonic origin for every collector, the channel and every track of this session.
        let origin = Instant::now();
        let created = chrono_free_date();
        let mut session = match DiagnosticSession::start(
            artifact_root,
            &format!("session-{}", wall_ms()),
            timeline_header(&created),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot start session: {e}");
                return 1;
            }
        };
        let mut processes = ProcessPackageCollector::new(
            WindowsProcessEnumerator,
            WindowsPackageIdentityProbe,
            SystemClock::with_origin(origin),
            targets.clone(),
        );
        let mut audio = AudioSessionMicCollector::new(
            WindowsSessionManager::new(),
            WindowsConsentStore,
            WindowsProcessEnumerator,
            WindowsPackageIdentityProbe,
            SystemClock::with_origin(origin),
            targets.clone(),
        );
        let mut server = Server::start(
            &ServerConfig {
                pinned_extension_id: pinned,
            },
            ChannelClock::with_origin(origin),
        );
        let mut listener = match LoopbackListener::bind() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("cannot bind the loopback listener: {e}");
                return 1;
            }
        };
        let _ = listener.set_nonblocking(true);
        let local_app_data = match std::env::var_os("LOCALAPPDATA") {
            Some(path) => PathBuf::from(path),
            None => {
                eprintln!("record needs LOCALAPPDATA to provision endpoint.json");
                return 2;
            }
        };
        let token = server.authenticator().token().to_hex();
        let mut endpoint_applier = WindowsAclApplier;
        if let Err(e) = (EndpointDescriptor {
            port: listener.port(),
            token: token.clone(),
        })
        .write(&local_app_data, &owner_sid, &mut endpoint_applier)
        {
            eprintln!("cannot provision endpoint.json: {e}");
            return 1;
        }
        if let Some(dir) = extension_dir {
            let mut applier = WindowsAclApplier;
            if let Err(e) = provision_extension(
                dir,
                listener.port(),
                &token,
                &tables.meeting_hosts(),
                &owner_sid,
                &mut applier,
            ) {
                eprintln!("cannot provision the extension: {e}");
                return 1;
            }
        }
        let mut resolver = WindowsPeerResolver;
        let mut snapshotter = WindowsProcessEnumerator;
        let tracks_root = artifact_root.join("tracks");
        let mut loopback: Option<TrackWorker> = None;
        let mut microphone: Option<TrackWorker> = None;
        let mut rounds = 0u64;
        let mut runtime_failed = false;
        'session: loop {
            rounds += 1;
            let snapshot = snapshotter.snapshot().unwrap_or_default();
            // Bound extension work per cycle. Four worst-case partial clients cost at most 200 ms;
            // capture itself remains continuously serviced by its dedicated worker threads.
            for _ in 0..4 {
                if !matches!(
                    listener.poll_once(&mut server, &mut resolver, &snapshot),
                    Ok(true)
                ) {
                    break;
                }
            }
            // Every drained channel signal is persisted, not one per round.
            let extension_signals = server.drain();
            let mut sources: Vec<&mut dyn SignalSource> = vec![&mut processes];
            if let Err(e) = session.observe_round(&mut sources) {
                eprintln!("persisting a signal failed: {e}");
                runtime_failed = true;
                break 'session;
            }
            if let Err(e) = persist_batch(audio.observe_batch(), |signal| session.append(signal)) {
                eprintln!("persisting a signal failed: {e}");
                runtime_failed = true;
                break 'session;
            }
            if let Err(e) = persist_batch(extension_signals, |signal| session.append(signal)) {
                eprintln!("persisting a signal failed: {e}");
                runtime_failed = true;
                break 'session;
            }
            // Open capture once a target process has an active capture session.
            let observed = audio
                .endpoint_observations()
                .iter()
                .next()
                .map(|o| (o.pid, o.endpoint_id.clone()));
            if let (None, Some((pid, endpoint))) = (&loopback, &observed) {
                loopback = Some(TrackWorker::loopback(*pid, tracks_root.clone(), origin));
                microphone = Some(TrackWorker::microphone(
                    endpoint.clone(),
                    tracks_root.clone(),
                    origin,
                ));
            }
            if let (Some(worker), Some((_, endpoint))) = (&microphone, &observed) {
                worker.update_endpoint(endpoint);
            }
            if loopback.as_ref().is_some_and(TrackWorker::is_failed)
                || microphone.as_ref().is_some_and(TrackWorker::is_failed)
            {
                eprintln!("capture worker failed");
                runtime_failed = true;
                break 'session;
            }
            std::thread::sleep(Duration::from_millis(500));
            if max_rounds.is_some_and(|m| rounds >= m) {
                break;
            }
        }
        if let Some(worker) = loopback.take() {
            runtime_failed |= !worker.finish().0;
        }
        if let Some(worker) = microphone.take() {
            let (ok, selection) = worker.finish();
            runtime_failed |= !ok;
            match selection {
                Some(selection) => match write_endpoint_selection(session.path(), &selection) {
                    Ok(path) => eprintln!("microphone endpoint selection: {}", path.display()),
                    Err(error) => {
                        eprintln!("cannot persist microphone endpoint selection: {error}");
                        runtime_failed = true;
                    }
                },
                None => {
                    eprintln!("microphone endpoint selection was not produced");
                    runtime_failed = true;
                }
            }
        }
        let mut table = tables.detector_table();
        match session.stop(&mut table) {
            Ok((end, _)) => {
                println!(
                    "timeline {} ({} signals); decisions {}",
                    end.timeline.display(),
                    end.signals_persisted,
                    end.decisions
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                );
                if runtime_failed {
                    1
                } else {
                    0
                }
            }
            Err(e) => {
                eprintln!("stop failed: {e}");
                1
            }
        }
    }

    fn wall_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// ISO date without a date crate: days since the epoch to a civil date.
    fn chrono_free_date() -> String {
        let days = (wall_ms() / 86_400_000) as i64;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    }
}
