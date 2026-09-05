//! A live diagnostic session: every observed signal is appended to the session's JSONL file
//! before the next one is read, so a session ended by stop, by cancel or by a crash keeps every
//! signal observed up to that point. `stop` additionally writes the decisions sidecar from one
//! `decide()` run over the persisted timeline; `cancel` does not. Neither discards the timeline.

use ma_detect::{decide, AdapterTable, DetectorConfig, DetectorOutput};
use ma_signal::{Signal, SignalSource, SignalTimeline, TimelineHeader};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// The `.labels.json` sidecar, in the existing fixture shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelsSidecar {
    pub timeline: String,
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub from_monotonic_ns: u64,
    pub to_monotonic_ns: u64,
    pub was_meeting: bool,
    pub note: String,
}

/// Where a finished session left its artefacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnd {
    pub timeline: PathBuf,
    /// Present after `stop`, absent after `cancel`.
    pub decisions: Option<PathBuf>,
    pub signals_persisted: usize,
}

pub struct DiagnosticSession {
    path: PathBuf,
    writer: BufWriter<File>,
    header: TimelineHeader,
    persisted: usize,
}

impl DiagnosticSession {
    /// Creates `<artifact_root>/<name>.jsonl` with the header line already durable.
    pub fn start(
        artifact_root: &Path,
        name: &str,
        header: TimelineHeader,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(artifact_root)?;
        let path = artifact_root.join(format!("{name}.jsonl"));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(
            serde_json::to_string(&header)
                .expect("header serializes")
                .as_bytes(),
        )?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer.get_ref().sync_data()?;
        Ok(Self {
            path,
            writer,
            header,
            persisted: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn signals_persisted(&self) -> usize {
        self.persisted
    }

    /// Appends one signal and makes it durable before returning.
    pub fn append(&mut self, signal: &Signal) -> std::io::Result<()> {
        self.writer.write_all(
            serde_json::to_string(signal)
                .expect("signal serializes")
                .as_bytes(),
        )?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        self.writer.get_ref().sync_data()?;
        self.persisted += 1;
        Ok(())
    }

    /// One observation round: reads at most one signal from each source, persisting each before
    /// the next source is read. Returns how many signals were observed. Never drains a source to
    /// exhaustion, which is what distinguishes a live loop from `SignalTimeline::merge`.
    pub fn observe_round(
        &mut self,
        sources: &mut [&mut dyn SignalSource],
    ) -> std::io::Result<usize> {
        let mut observed = 0;
        for source in sources.iter_mut() {
            if let Some(signal) = source.next_signal() {
                self.append(&signal)?;
                observed += 1;
            }
        }
        Ok(observed)
    }

    /// Re-reads the persisted timeline; the file, not memory, is the truth.
    pub fn persisted_timeline(&self) -> std::io::Result<SignalTimeline> {
        let text = std::fs::read_to_string(&self.path)?;
        SignalTimeline::from_jsonl(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Ends the session and writes `<timeline>.decisions.json` from one `decide()` run over the
    /// persisted timeline.
    pub fn stop(self, table: &mut AdapterTable) -> std::io::Result<(SessionEnd, DetectorOutput)> {
        let timeline = self.persisted_timeline()?;
        let output = decide(&timeline, &DetectorConfig::default(), table);
        let decisions = decisions_path(&self.path);
        std::fs::write(&decisions, output.to_canonical_json())?;
        Ok((
            SessionEnd {
                timeline: self.path.clone(),
                decisions: Some(decisions),
                signals_persisted: self.persisted,
            },
            output,
        ))
    }

    /// Ends the session without a decisions sidecar. The timeline is kept as persisted.
    pub fn cancel(self) -> SessionEnd {
        SessionEnd {
            timeline: self.path.clone(),
            decisions: None,
            signals_persisted: self.persisted,
        }
    }

    pub fn header(&self) -> &TimelineHeader {
        &self.header
    }
}

/// `<stem>.decisions.json` next to the timeline.
pub fn decisions_path(timeline: &Path) -> PathBuf {
    sidecar(timeline, "decisions.json")
}

/// `<stem>.labels.json` next to the timeline.
pub fn labels_path(timeline: &Path) -> PathBuf {
    sidecar(timeline, "labels.json")
}

fn sidecar(timeline: &Path, suffix: &str) -> PathBuf {
    let stem = timeline
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    timeline.with_file_name(format!("{stem}.{suffix}"))
}

/// The `label` command: attaches a `was_meeting` entry for a time range to the timeline's
/// `.labels.json`, creating the sidecar in the existing shape when absent (FR-109).
pub fn label_timeline(
    timeline: &Path,
    from_ns: u64,
    to_ns: u64,
    was_meeting: bool,
    note: &str,
) -> std::io::Result<PathBuf> {
    if !timeline.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("timeline {} does not exist", timeline.display()),
        ));
    }
    if to_ns < from_ns {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "label range ends before it starts",
        ));
    }
    let path = labels_path(timeline);
    let mut sidecar: LabelsSidecar = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LabelsSidecar {
            timeline: timeline
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            labels: Vec::new(),
        },
        Err(e) => return Err(e),
    };
    sidecar.labels.push(Label {
        from_monotonic_ns: from_ns,
        to_monotonic_ns: to_ns,
        was_meeting,
        note: note.to_string(),
    });
    let mut text = serde_json::to_string_pretty(&sidecar).expect("sidecar serializes");
    text.push('\n');
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{timeline_header, AdapterTables, Command};
    use ma_core_types::id::TypedId;
    use ma_core_types::SignalId;
    use ma_signal::adapter::TableAdapter;
    use ma_signal::{Authority, ObservedAt, Payload, SignalKind, Subject, SCHEMA_VERSION};

    fn signal(kind: SignalKind, pid: u32, t: u64) -> Signal {
        Signal {
            signal_id: SignalId::new(),
            source_id: "os.process".into(),
            kind,
            subject: Subject::Process {
                pid,
                image_name: "example-desk.exe".into(),
                package_family_name: None,
            },
            observed_at: ObservedAt {
                monotonic_ns: t,
                wall_utc_ms: 1_756_857_600_000 + (t / 1_000_000) as i64,
            },
            payload: Payload::default(),
            authority: Authority::Os,
            schema_version: SCHEMA_VERSION,
        }
    }

    /// A source that yields one signal per call and records how many were taken, so a test can
    /// see whether the harness drained it.
    struct Metered {
        signals: Vec<Signal>,
        taken: usize,
    }
    impl SignalSource for Metered {
        fn source_id(&self) -> &str {
            "metered"
        }
        fn next_signal(&mut self) -> Option<Signal> {
            let s = self.signals.get(self.taken).cloned();
            if s.is_some() {
                self.taken += 1;
            }
            s
        }
    }

    fn desk_table() -> AdapterTables {
        AdapterTables::from_adapters(vec![TableAdapter::from_toml(
            r#"
id = "desk-a"
class = "desktop"
evidence_weight = 2
corroboration = { microphone = true, tab = false }
process_images = ["example-desk.exe"]
"#,
        )
        .unwrap()])
    }

    #[test]
    fn cancelled_session_keeps_its_partial_timeline() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = Metered {
            signals: vec![
                signal(SignalKind::ProcessStarted, 100, 1_000_000_000),
                signal(SignalKind::MicCaptureStarted, 100, 2_000_000_000),
                signal(SignalKind::MicCaptureStopped, 100, 3_000_000_000),
                signal(SignalKind::ProcessStopped, 100, 4_000_000_000),
            ],
            taken: 0,
        };
        let mut session =
            DiagnosticSession::start(dir.path(), "session-1", timeline_header("2026-09-04"))
                .unwrap();
        // Two rounds: two signals persisted, each before the next read.
        for expected in 1..=2 {
            assert_eq!(session.observe_round(&mut [&mut source]).unwrap(), 1);
            let on_disk = session.persisted_timeline().unwrap();
            assert_eq!(
                on_disk.signals().len(),
                expected,
                "persisted before the next read"
            );
        }
        assert_eq!(
            source.taken, 2,
            "the live loop never drains a source to exhaustion"
        );
        let end = session.cancel();
        assert_eq!(end.decisions, None, "cancel writes no decisions sidecar");
        assert_eq!(end.signals_persisted, 2);
        let text = std::fs::read_to_string(&end.timeline).unwrap();
        let timeline = SignalTimeline::from_jsonl(&text).unwrap();
        assert_eq!(timeline.signals().len(), 2);
        assert_eq!(timeline.signals()[1].kind, SignalKind::MicCaptureStarted);
        assert!(!decisions_path(&end.timeline).exists());
    }

    #[test]
    fn session_end_writes_decisions_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = Metered {
            signals: vec![
                signal(SignalKind::ProcessStarted, 100, 1_000_000_000),
                signal(SignalKind::MicCaptureStarted, 100, 2_000_000_000),
                signal(SignalKind::MicCaptureStopped, 100, 3_000_000_000),
            ],
            taken: 0,
        };
        let mut session =
            DiagnosticSession::start(dir.path(), "session-2", timeline_header("2026-09-04"))
                .unwrap();
        while session.observe_round(&mut [&mut source]).unwrap() > 0 {}
        let tables = desk_table();
        let mut table = tables.detector_table();
        let (end, output) = session.stop(&mut table).unwrap();
        let decisions = end.decisions.expect("stop writes the sidecar");
        assert!(decisions.ends_with("session-2.decisions.json"));
        let text = std::fs::read_to_string(&decisions).unwrap();
        assert_eq!(
            text,
            output.to_canonical_json(),
            "one decide() run, written verbatim"
        );
        assert!(output
            .decisions
            .iter()
            .any(|d| d.outcome.is_determinate_start()));
        // The sidecar is derived from the persisted timeline, so replaying it reproduces it.
        let persisted =
            SignalTimeline::from_jsonl(&std::fs::read_to_string(&end.timeline).unwrap()).unwrap();
        let again = decide(
            &persisted,
            &DetectorConfig::default(),
            &mut tables.detector_table(),
        );
        assert_eq!(again.to_canonical_json(), text);
    }

    #[test]
    fn label_command_writes_labels_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let session =
            DiagnosticSession::start(dir.path(), "session-3", timeline_header("2026-09-04"))
                .unwrap();
        let end = session.cancel();
        let path = label_timeline(
            &end.timeline,
            1_800_000_000,
            2_500_000_000,
            true,
            "confirmed",
        )
        .unwrap();
        assert!(path.ends_with("session-3.labels.json"));
        let sidecar: LabelsSidecar =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(sidecar.timeline, "session-3.jsonl");
        assert_eq!(sidecar.labels.len(), 1);
        assert!(sidecar.labels[0].was_meeting);
        // The same shape as the committed fixtures' sidecars.
        let fixture: LabelsSidecar = serde_json::from_str(include_str!(
            "../../../../fixtures/signal-timelines/browser-tab-with-mic.labels.json"
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&fixture.labels[0])
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            serde_json::to_value(&sidecar.labels[0])
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        // A second label appends; a reversed range is refused.
        label_timeline(
            &end.timeline,
            3_000_000_000,
            3_500_000_000,
            false,
            "left the call",
        )
        .unwrap();
        let sidecar: LabelsSidecar =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(sidecar.labels.len(), 2);
        assert!(label_timeline(&end.timeline, 5, 4, true, "").is_err());
        assert!(label_timeline(&dir.path().join("missing.jsonl"), 1, 2, true, "").is_err());
    }
}
