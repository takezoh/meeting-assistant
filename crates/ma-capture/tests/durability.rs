//! Chunk durability, timeline segments and consolidation, all driven by the synthetic source.

use ma_capture::*;
use ma_core_types::id::TypedId;
use ma_core_types::timeline::{CaptureMode, ContaminationRisk, GapReason, TrackOrigin};
use ma_core_types::TrackId;
use std::path::Path;

fn record(dir: &Path, total_samples: u64, fs: &mut dyn ChunkFs) -> ChunkWriter {
    let mut source = SyntheticSource::new(SAMPLE_RATE, total_samples, 16_000);
    let mut writer = ChunkWriter::open(dir, TrackId::new(), "mic", source.origin()).unwrap();
    loop {
        match source.next() {
            SourceEvent::Samples(s) => {
                writer.push(&s);
                let _ = writer.drain(fs);
            }
            SourceEvent::FormatChanged(origin) => {
                writer = writer
                    .open_successor(fs, dir.parent().unwrap(), TrackId::new(), origin, 0)
                    .unwrap();
            }
            SourceEvent::Ended => break,
        }
    }
    writer.finish(fs).unwrap();
    writer
}

fn wav_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".wav"))
        .collect();
    v.sort();
    v
}

#[test]
fn directory_is_truth_manifest_is_cache() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    // 95 s: three full chunks and a 5 s tail
    let writer = record(&track, 95 * SAMPLE_RATE as u64, &mut RealFs);
    assert_eq!(
        wav_files(&track),
        ["000000.wav", "000001.wav", "000002.wav", "000003.wav"]
    );
    let m = writer.manifest();
    assert_eq!(m.chunks.len(), 4);
    assert_eq!(m.chunks[3].start_sample, 1_440_000);
    assert_eq!(m.chunks[3].len_samples, 80_000);
    // schema conformance of what was written
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/artifact/chunk-manifest.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(track.join(MANIFEST_FILE)).unwrap()).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    // direction 1: the manifest names a file that is gone → explicit gap, no renumbering
    std::fs::remove_file(track.join("000001.wav")).unwrap();
    // direction 2: a file exists that the manifest does not know → adopted
    let mut manifest = ChunkManifest::load(&track).unwrap().unwrap();
    let dropped = manifest.chunks.remove(2);
    assert_eq!(dropped.seq, 2);
    manifest.save(&track).unwrap();
    // a killed writer leaves a .part with 12 s of audio → repaired; an empty .part → discarded and gapped
    let twelve: Vec<i16> = (0..12 * SAMPLE_RATE as u64)
        .map(SyntheticSource::sample)
        .collect();
    let mut part = ma_capture::wav::encode(SAMPLE_RATE, 1, &twelve);
    part.push(0x11); // a trailing incomplete frame byte
    std::fs::write(track.join("000004.wav.part"), &part).unwrap();
    std::fs::write(
        track.join("000005.wav.part"),
        ma_capture::wav::header(SAMPLE_RATE, 1, 0),
    )
    .unwrap();
    let report = recover(&track, 5).unwrap();
    assert_eq!(report.gapped, [1]);
    assert_eq!(report.adopted, [2]);
    assert_eq!(report.repaired, [4]);
    assert_eq!(report.discarded, [5]);
    let m = report.manifest.unwrap();
    let seqs: Vec<u32> = m.chunks.iter().map(|c| c.seq).collect();
    assert_eq!(
        seqs,
        [0, 2, 3, 4],
        "sequence numbers are dense-or-gapped, never renumbered"
    );
    let gap = m
        .gaps
        .iter()
        .find(|g| g.reason == GapReason::ChunkLost)
        .expect("the lost chunk is an explicit gap");
    assert_eq!((gap.from_sample, gap.to_sample), (480_000, 960_000));
    assert_eq!(
        m.chunks.iter().find(|c| c.seq == 2).unwrap().start_sample,
        960_000,
        "the adopted chunk keeps its true position after the gap"
    );
    let repaired = std::fs::read(track.join("000004.wav")).unwrap();
    assert_eq!(
        ma_capture::wav::decode(&repaired).unwrap().samples.len(),
        12 * SAMPLE_RATE as usize
    );
    assert!(!track.join("000005.wav.part").exists());
    assert!(m
        .events
        .iter()
        .any(|e| e.kind == ManifestEventKind::ChunkRepaired));
    // idempotent: a second recovery changes nothing
    let again = recover(&track, 6).unwrap();
    assert!(again.gapped.is_empty() && again.adopted.is_empty() && again.repaired.is_empty());
}

/// A filesystem that refuses every write for a while: the writer must drop audio loudly, not block.
struct StallingFs {
    stalled: bool,
    inner: RealFs,
    writes: u32,
}
impl ChunkFs for StallingFs {
    fn write_part(&mut self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        if self.stalled {
            return Err(FsError::Stalled);
        }
        self.writes += 1;
        self.inner.write_part(path, bytes)
    }
    fn rename_in(&mut self, part: &Path, final_path: &Path) -> Result<(), FsError> {
        self.inner.rename_in(part, final_path)
    }
    fn save_manifest(&mut self, dir: &Path, manifest: &ChunkManifest) -> Result<(), FsError> {
        self.inner.save_manifest(dir, manifest)
    }
}

#[test]
fn stalling_filesystem_yields_gap_not_stall() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    let mut fs = StallingFs {
        stalled: true,
        inner: RealFs,
        writes: 0,
    };
    let mut source = SyntheticSource::new(SAMPLE_RATE, 150 * SAMPLE_RATE as u64, 16_000);
    let mut writer = ChunkWriter::open(&track, TrackId::new(), "mic", source.origin()).unwrap();
    // 150 s of audio arrive while the disk is stalled: the callback is never blocked
    let mut pushes = 0;
    while let SourceEvent::Samples(s) = source.next() {
        writer.push(&s);
        let _ = writer.drain(&mut fs);
        pushes += 1;
    }
    assert_eq!(pushes, 150);
    assert!(
        writer.queued_samples() <= QUEUE_CAP_SAMPLES,
        "memory is bounded"
    );
    assert_eq!(fs.writes, 0);
    let dropped: u64 = writer
        .events
        .iter()
        .filter_map(|e| match e {
            WriterEvent::Degraded {
                reason: DegradedReason::DiskBackpressure,
                dropped_samples,
            } => Some(*dropped_samples),
            _ => None,
        })
        .sum();
    assert_eq!(
        dropped,
        90 * SAMPLE_RATE as u64,
        "everything beyond the 60 s queue was dropped"
    );
    assert!(
        !writer.manifest().gaps.is_empty(),
        "the loss is an explicit gap"
    );
    // the disk comes back: the queued 60 s drain, the gap sits after them
    fs.stalled = false;
    writer.finish(&mut fs).unwrap();
    assert_eq!(wav_files(&track), ["000000.wav", "000001.wav"]);
    let m = writer.manifest();
    let gap = &m.gaps[0];
    assert_eq!(gap.from_sample, 960_000);
    assert_eq!(gap.to_sample - gap.from_sample, 90 * SAMPLE_RATE as u64);
    assert_eq!(gap.reason, GapReason::CaptureInterrupted);
}

#[test]
fn device_format_change_opens_new_segment() {
    // "segment" in the verification id: the track that continues after a device format change. It is
    // a successor track with its own identifier, directory, origin and sample space (matching
    // TrackSegment::open_successor and the store's one-origin-per-track row); old positions are never
    // reinterpreted at the new rate.
    let dir = tempfile::tempdir().unwrap();
    let tracks_root = dir.path().join("chunks");
    let first_id = TrackId::new();
    let mut source = SyntheticSource::new(SAMPLE_RATE, 70 * SAMPLE_RATE as u64, 16_000);
    let reconnected = TrackOrigin {
        start_wall_utc_ms: 1_756_857_645_000,
        start_monotonic_ns: 46_000_000_000,
        sample_rate: 48_000,
        channels: 1,
        capture_mode: CaptureMode::Device,
        contamination_risk: ContaminationRisk::None,
    };
    source = source.with_format_change(45 * SAMPLE_RATE as u64, reconnected.clone());
    let mut writer = ChunkWriter::open(
        &tracks_root.join(first_id.to_string()),
        first_id,
        "mic",
        source.origin(),
    )
    .unwrap();
    let mut fs = RealFs;
    let mut successor_id = None;
    loop {
        match source.next() {
            SourceEvent::Samples(s) => {
                writer.push(&s);
                writer.drain(&mut fs).unwrap();
            }
            SourceEvent::FormatChanged(origin) => {
                let next_id = TrackId::new();
                successor_id = Some(next_id);
                writer = writer
                    .open_successor(&mut fs, &tracks_root, next_id, origin, 45_000)
                    .unwrap();
            }
            SourceEvent::Ended => break,
        }
    }
    writer.finish(&mut fs).unwrap();
    let second_id = successor_id.expect("a successor was opened");
    let first = ChunkManifest::load(&tracks_root.join(first_id.to_string()))
        .unwrap()
        .unwrap();
    let second = ChunkManifest::load(&tracks_root.join(second_id.to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(first.successor, Some(second_id));
    assert_eq!(second.predecessor, Some(first_id));
    assert_eq!(second.origin, reconnected);
    assert_eq!(first.origin.sample_rate, 16_000);
    assert!(first
        .events
        .iter()
        .any(|e| e.kind == ManifestEventKind::SuccessorOpened));
    // the first track holds 45 s at 16 kHz (30 s + 15 s tail flushed at the change); the successor
    // starts its own sample space at 0 in the new format
    assert_eq!(first.end_sample(), 45 * SAMPLE_RATE as u64);
    assert_eq!(first.chunks[1].len_samples, 15 * SAMPLE_RATE as u64);
    assert_eq!(second.chunks[0].start_sample, 0);
    assert_eq!(second.end_sample(), 25 * SAMPLE_RATE as u64);
    assert_eq!(
        ma_capture::wav::decode(
            &std::fs::read(
                tracks_root
                    .join(second_id.to_string())
                    .join(&second.chunks[0].file)
            )
            .unwrap()
        )
        .unwrap()
        .sample_rate,
        48_000,
        "written in the new format"
    );
    assert_eq!(
        ma_capture::wav::decode(
            &std::fs::read(
                tracks_root
                    .join(first_id.to_string())
                    .join(&first.chunks[0].file)
            )
            .unwrap()
        )
        .unwrap()
        .sample_rate,
        16_000,
        "old chunks are not reinterpreted"
    );
    // each track consolidates to tracks/<track_id>.flac at its own rate — the store's layout
    let out = dir.path().join("tracks");
    let r1 = consolidate(&tracks_root.join(first_id.to_string()), &out, 1).unwrap();
    let r2 = consolidate(&tracks_root.join(second_id.to_string()), &out, 2).unwrap();
    assert!(r1.flac_file.ends_with(format!("{first_id}.flac")));
    assert!(r2.flac_file.ends_with(format!("{second_id}.flac")));
    let (rate1, _, s1) =
        ma_capture::consolidate::decode_flac(&std::fs::read(&r1.flac_file).unwrap()).unwrap();
    let (rate2, _, s2) =
        ma_capture::consolidate::decode_flac(&std::fs::read(&r2.flac_file).unwrap()).unwrap();
    assert_eq!((rate1, s1.len()), (16_000, 45 * SAMPLE_RATE as usize));
    assert_eq!((rate2, s2.len()), (48_000, 25 * SAMPLE_RATE as usize));
    assert_eq!(
        s2[0],
        SyntheticSource::sample(45 * SAMPLE_RATE as u64),
        "the successor starts with the first sample after the change"
    );
}

#[test]
fn flac_decodes_sample_identical() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    let out = dir.path().join("tracks");
    let writer = record(&track, 95 * SAMPLE_RATE as u64, &mut RealFs);
    let expected: Vec<i16> = (0..95 * SAMPLE_RATE as u64)
        .map(SyntheticSource::sample)
        .collect();
    drop(writer);
    let report = consolidate(&track, &out, 1).unwrap();
    assert!(report.encoded);
    assert_eq!(report.samples, 1_520_000);
    assert_eq!(report.chunks_deleted, 4);
    let (rate, channels, decoded) =
        ma_capture::consolidate::decode_flac(&std::fs::read(&report.flac_file).unwrap()).unwrap();
    assert_eq!((rate, channels), (SAMPLE_RATE, 1));
    assert_eq!(decoded, expected, "sample-identical");
    assert!(
        wav_files(&track).is_empty(),
        "chunks are deleted only after verification"
    );
    let m = ChunkManifest::load(&track).unwrap().unwrap();
    assert_eq!(
        m.consolidated_file,
        Some(format!("{}.flac", m.track)),
        "named by the track id, never by a role"
    );
    assert!(report.flac_file.ends_with(format!("{}.flac", m.track)));
    let kinds: Vec<ManifestEventKind> = m.events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            ManifestEventKind::Consolidated,
            ManifestEventKind::ChunksDeleted
        ],
        "the deletion is itself a recorded event, after the consolidation record"
    );
    // a gap is rendered as silence and kept as a record
    let dir2 = tempfile::tempdir().unwrap();
    let track2 = dir2.path().join("mic");
    record(&track2, 95 * SAMPLE_RATE as u64, &mut RealFs);
    std::fs::remove_file(track2.join("000001.wav")).unwrap();
    recover(&track2, 2).unwrap();
    let report = consolidate(&track2, &dir2.path().join("tracks"), 3).unwrap();
    let (_, _, decoded) =
        ma_capture::consolidate::decode_flac(&std::fs::read(&report.flac_file).unwrap()).unwrap();
    assert_eq!(decoded.len(), 1_520_000);
    assert!(decoded[480_000..960_000].iter().all(|s| *s == 0));
    assert_eq!(
        decoded[960_000],
        SyntheticSource::sample(960_000),
        "positions after the gap are preserved"
    );
    assert_eq!(
        ChunkManifest::load(&track2).unwrap().unwrap().gaps.len(),
        1,
        "the gap record survives consolidation"
    );
}

#[test]
fn crash_between_verify_and_delete_is_idempotent() {
    for crash in [
        CrashPoint::AfterEncode,
        CrashPoint::AfterVerify,
        CrashPoint::AfterRename,
        CrashPoint::AfterRecord,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("mic");
        let out = dir.path().join("tracks");
        record(&track, 65 * SAMPLE_RATE as u64, &mut RealFs);
        assert_eq!(
            consolidate_with(&track, &out, &FlacEncoder, Some(crash), 1),
            Err(ConsolidateError::Crashed(crash))
        );
        assert_eq!(
            wav_files(&track).len(),
            3,
            "chunks survive a crash at {crash:?}"
        );
        let flacs_before = std::fs::read_dir(&out).unwrap().count();
        let report = consolidate(&track, &out, 2).unwrap_or_else(|e| {
            panic!(
                "{crash:?}: {e:?}; track={:?} out={:?}",
                wav_files(&track),
                std::fs::read_dir(&out)
                    .unwrap()
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            )
        });
        assert_eq!(
            report.encoded,
            crash != CrashPoint::AfterRecord,
            "after the record the re-run only completes the deletion"
        );
        assert_eq!(report.chunks_deleted, 3);
        assert_eq!(
            std::fs::read_dir(&out).unwrap().count(),
            1,
            "one FLAC, never a second (had {flacs_before} before)"
        );
        assert!(wav_files(&track).is_empty());
        let (_, _, decoded) =
            ma_capture::consolidate::decode_flac(&std::fs::read(&report.flac_file).unwrap())
                .unwrap();
        assert_eq!(decoded.len(), 65 * SAMPLE_RATE as usize);
        // and a third run is a no-op
        let again = consolidate(&track, &out, 3).unwrap();
        assert!(!again.encoded && again.chunks_deleted == 0);
    }
}

/// An encoder configured for the wrong channel count: it silently produces stereo.
struct StereoEncoder;
impl Encoder for StereoEncoder {
    fn encode(&self, sample_rate: u32, _channels: u16, samples: &[i16]) -> Result<Vec<u8>, String> {
        let doubled: Vec<i16> = samples.iter().flat_map(|s| [*s, *s]).collect();
        FlacEncoder.encode(sample_rate, 2, &doubled)
    }
}

#[test]
fn verification_mismatch_preserves_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    let out = dir.path().join("tracks");
    record(&track, 65 * SAMPLE_RATE as u64, &mut RealFs);
    let err = consolidate_with(&track, &out, &StereoEncoder, None, 1).unwrap_err();
    assert!(
        matches!(
            err,
            ConsolidateError::VerificationMismatch {
                expected_samples: 1_040_000,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(wav_files(&track).len(), 3, "chunks are the archival form");
    assert!(
        std::fs::read_dir(&out).unwrap().next().is_none(),
        "the FLAC is discarded"
    );
    let m = ChunkManifest::load(&track).unwrap().unwrap();
    assert!(m.consolidated_file.is_none());
    assert!(
        m.events
            .iter()
            .any(|e| e.kind == ManifestEventKind::ConsolidationFailed),
        "marked consolidation_failed"
    );
}

/// A filesystem whose disk fills: writes fail with the disk-full error until it drains.
struct FullDiskFs {
    full: bool,
    inner: RealFs,
}
impl ChunkFs for FullDiskFs {
    fn write_part(&mut self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        if self.full {
            return Err(FsError::DiskFull);
        }
        self.inner.write_part(path, bytes)
    }
    fn rename_in(&mut self, part: &Path, final_path: &Path) -> Result<(), FsError> {
        self.inner.rename_in(part, final_path)
    }
    fn save_manifest(&mut self, dir: &Path, manifest: &ChunkManifest) -> Result<(), FsError> {
        self.inner.save_manifest(dir, manifest)
    }
}

#[test]
fn disk_full_is_surfaced_as_disk_full() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    let mut fs = FullDiskFs {
        full: true,
        inner: RealFs,
    };
    let mut source = SyntheticSource::new(SAMPLE_RATE, 100 * SAMPLE_RATE as u64, 16_000);
    let mut writer = ChunkWriter::open(&track, TrackId::new(), "mic", source.origin()).unwrap();
    while let SourceEvent::Samples(s) = source.next() {
        writer.push(&s);
        let _ = writer.drain(&mut fs);
    }
    let reasons: std::collections::BTreeSet<String> = writer
        .events
        .iter()
        .filter_map(|e| match e {
            WriterEvent::Degraded { reason, .. } => Some(format!("{reason:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasons.into_iter().collect::<Vec<_>>(),
        ["DiskFull"],
        "drops while the disk is full are attributed to the full disk"
    );
    assert!(
        is_disk_full(&std::io::Error::from_raw_os_error(112)),
        "Windows ERROR_DISK_FULL"
    );
    assert!(
        is_disk_full(&std::io::Error::from_raw_os_error(39)),
        "Windows ERROR_HANDLE_DISK_FULL"
    );
    assert!(
        is_disk_full(&std::io::Error::from_raw_os_error(28)),
        "ENOSPC"
    );
    assert!(!is_disk_full(&std::io::Error::from_raw_os_error(13)));
}

#[test]
fn two_overflows_keep_every_position_exact() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("mic");
    let mut fs = StallingFs {
        stalled: true,
        inner: RealFs,
        writes: 0,
    };
    let origin = SyntheticSource::new(SAMPLE_RATE, 0, 1).origin();
    let mut writer = ChunkWriter::open(&track, TrackId::new(), "mic", origin).unwrap();
    let mut produced = 0u64;
    let mut feed = |writer: &mut ChunkWriter, fs: &mut StallingFs, seconds: u64| {
        for _ in 0..seconds {
            let samples: Vec<i16> = (0..SAMPLE_RATE as u64)
                .map(|k| SyntheticSource::sample(produced + k))
                .collect();
            produced += SAMPLE_RATE as u64;
            writer.push(&samples);
            let _ = writer.drain(fs);
        }
    };
    // stall: 60 s queued, 30 s dropped (gap A)
    feed(&mut writer, &mut fs, 90);
    // the disk returns for one second of audio (one chunk drains), then stalls again while the
    // queue refills and overflows (gap B)
    fs.stalled = false;
    feed(&mut writer, &mut fs, 1);
    fs.stalled = true;
    feed(&mut writer, &mut fs, 80);
    fs.stalled = false;
    writer.finish(&mut fs).unwrap();
    let m = writer.manifest();
    assert_eq!(m.gaps.len(), 2, "two distinct gaps: {:?}", m.gaps);
    // chunks ∪ gaps tile the track with no overlap and no shift
    let mut spans: Vec<(u64, u64)> = m
        .chunks
        .iter()
        .map(|c| (c.start_sample, c.start_sample + c.len_samples))
        .chain(m.gaps.iter().map(|g| (g.from_sample, g.to_sample)))
        .collect();
    spans.sort();
    let mut cursor = 0;
    for (from, to) in &spans {
        assert_eq!(*from, cursor, "hole or overlap at {from}: {spans:?}");
        cursor = *to;
    }
    assert_eq!(
        cursor, produced,
        "every produced sample has exactly one position"
    );
    // and the audio in each chunk is the audio that was produced at that position
    for chunk in &m.chunks {
        let decoded =
            ma_capture::wav::decode(&std::fs::read(track.join(&chunk.file)).unwrap()).unwrap();
        assert_eq!(
            decoded.samples[0],
            SyntheticSource::sample(chunk.start_sample),
            "chunk {} starts with the sample of its position",
            chunk.seq
        );
    }
}

#[test]
fn missing_manifest_is_rebuilt_from_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let track_id = TrackId::new();
    let track = dir.path().join(track_id.to_string());
    let mut source = SyntheticSource::new(SAMPLE_RATE, 65 * SAMPLE_RATE as u64, 16_000);
    let mut writer = ChunkWriter::open(&track, track_id, "mic", source.origin()).unwrap();
    assert!(
        track.join(MANIFEST_FILE).exists(),
        "a new track has a manifest before any chunk"
    );
    while let SourceEvent::Samples(s) = source.next() {
        writer.push(&s);
        writer.drain(&mut RealFs).unwrap();
    }
    writer.finish(&mut RealFs).unwrap();
    drop(writer);
    // the manifest is corrupt: the audio must not become unreachable
    std::fs::write(track.join(MANIFEST_FILE), b"{ not json").unwrap();
    let report = recover(&track, 5).unwrap();
    assert!(report.rebuilt);
    assert_eq!(report.adopted, [0, 1, 2]);
    let m = report.manifest.unwrap();
    assert_eq!(
        m.track, track_id,
        "the track id comes from the directory name"
    );
    assert_eq!(m.role, "recovered");
    assert_eq!(
        m.origin.sample_rate, SAMPLE_RATE,
        "format from the chunk headers"
    );
    assert_eq!(m.chunks[2].start_sample, 960_000);
    assert!(m
        .events
        .iter()
        .any(|e| e.kind == ManifestEventKind::ManifestRebuilt));
    // absent is rebuilt too, and the rebuilt manifest consolidates
    std::fs::remove_file(track.join(MANIFEST_FILE)).unwrap();
    assert!(recover(&track, 6).unwrap().rebuilt);
    let report = consolidate(&track, &dir.path().join("tracks"), 7).unwrap();
    assert_eq!(report.samples, 65 * SAMPLE_RATE as u64);
}
