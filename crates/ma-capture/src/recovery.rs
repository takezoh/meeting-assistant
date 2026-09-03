//! Directory-truth recovery: adopt every `<seq>.wav` present, turn manifest records for absent files
//! into explicit gaps, repair or discard `.part` files, never renumber. A missing or corrupt manifest
//! is rebuilt from the directory, because the audio on disk is the truth and must never be unreachable.

use crate::manifest::{ChunkManifest, ChunkRecord, ManifestEvent, ManifestEventKind};
use crate::wav;
use ma_core_types::timeline::{CaptureMode, ContaminationRisk, Gap, GapReason, TrackOrigin};
use ma_core_types::TrackId;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub adopted: Vec<u32>,
    pub gapped: Vec<u32>,
    pub repaired: Vec<u32>,
    pub discarded: Vec<u32>,
    /// The manifest was absent or unreadable and has been rebuilt from the directory.
    pub rebuilt: bool,
    pub manifest: Option<ChunkManifest>,
}

fn scan(track_dir: &Path) -> std::io::Result<BTreeMap<u32, (bool, bool)>> {
    let mut present: BTreeMap<u32, (bool, bool)> = BTreeMap::new();
    for entry in std::fs::read_dir(track_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(seq) = name
            .strip_suffix(".wav.part")
            .and_then(|s| s.parse::<u32>().ok())
        {
            present.entry(seq).or_default().1 = true;
        } else if let Some(seq) = name
            .strip_suffix(".wav")
            .and_then(|s| s.parse::<u32>().ok())
        {
            present.entry(seq).or_default().0 = true;
        }
    }
    Ok(present)
}

/// A manifest rebuilt from the directory alone: the track id is the directory name, the origin's
/// format comes from the first chunk's header, and the role and clocks are unknown.
fn rebuild(
    track_dir: &Path,
    present: &BTreeMap<u32, (bool, bool)>,
    now_ms: u64,
) -> std::io::Result<ChunkManifest> {
    let track: TrackId = track_dir
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| {
            use ma_core_types::id::TypedId;
            TrackId::new()
        });
    let mut origin = TrackOrigin {
        start_wall_utc_ms: 0,
        start_monotonic_ns: 0,
        sample_rate: 16_000,
        channels: 1,
        capture_mode: CaptureMode::Device,
        contamination_risk: ContaminationRisk::None,
    };
    for (seq, (has_wav, _)) in present {
        if *has_wav {
            if let Ok(decoded) = wav::decode(&std::fs::read(
                track_dir.join(ChunkManifest::chunk_file(*seq)),
            )?) {
                origin.sample_rate = decoded.sample_rate;
                origin.channels = decoded.channels;
                break;
            }
        }
    }
    let mut manifest = ChunkManifest::new(track, "recovered", origin);
    manifest.events.push(ManifestEvent {
        kind: ManifestEventKind::ManifestRebuilt,
        at_ms: now_ms,
        seq: None,
        file: None,
        samples: None,
    });
    Ok(manifest)
}

/// Reconcile the chunk directory and its manifest in both directions.
pub fn recover(track_dir: &Path, now_ms: u64) -> std::io::Result<RecoveryReport> {
    let mut report = RecoveryReport::default();
    if !track_dir.is_dir() {
        return Ok(report);
    }
    let mut present = scan(track_dir)?;
    let mut manifest = match ChunkManifest::load(track_dir) {
        Ok(Some(m)) => m,
        Ok(None) | Err(_) => {
            if present.is_empty() {
                return Ok(report);
            }
            report.rebuilt = true;
            rebuild(track_dir, &present, now_ms)?
        }
    };
    // .part files: repair if at least one complete frame, else discard
    let part_seqs: Vec<u32> = present
        .iter()
        .filter(|(_, p)| p.1)
        .map(|(s, _)| *s)
        .collect();
    for seq in part_seqs {
        let part = track_dir.join(format!("{:06}.wav.part", seq));
        let bytes = std::fs::read(&part)?;
        match wav::repair_part(&bytes) {
            Some(fixed) => {
                let final_path = track_dir.join(ChunkManifest::chunk_file(seq));
                std::fs::write(&final_path, &fixed)?;
                std::fs::remove_file(&part)?;
                report.repaired.push(seq);
                manifest.events.push(ManifestEvent {
                    kind: ManifestEventKind::ChunkRepaired,
                    at_ms: now_ms,
                    seq: Some(seq),
                    file: Some(ChunkManifest::chunk_file(seq)),
                    samples: Some(((fixed.len() - wav::HEADER_LEN) / 2) as u64),
                });
                present.insert(seq, (true, false));
            }
            None => {
                std::fs::remove_file(&part)?;
                report.discarded.push(seq);
                manifest.events.push(ManifestEvent {
                    kind: ManifestEventKind::ChunkDiscarded,
                    at_ms: now_ms,
                    seq: Some(seq),
                    file: None,
                    samples: None,
                });
            }
        }
    }
    // manifest → directory: a record naming an absent file becomes a gap
    let mut kept = Vec::new();
    for record in manifest.chunks.drain(..) {
        if present.get(&record.seq).is_some_and(|p| p.0) {
            kept.push(record);
        } else {
            report.gapped.push(record.seq);
            manifest.gaps.push(Gap {
                from_sample: record.start_sample,
                to_sample: record.start_sample + record.len_samples,
                reason: GapReason::ChunkLost,
            });
        }
    }
    manifest.chunks = kept;
    // directory → manifest: a present file without a record is adopted at its sequence position
    for (&seq, &(has_wav, _)) in &present {
        if !has_wav || manifest.chunks.iter().any(|c| c.seq == seq) {
            continue;
        }
        let bytes = std::fs::read(track_dir.join(ChunkManifest::chunk_file(seq)))?;
        let Ok(decoded) = wav::decode(&bytes) else {
            continue;
        };
        let len = decoded.samples.len() as u64;
        let start_sample = position_for(&manifest, seq);
        manifest.chunks.push(ChunkRecord {
            seq,
            file: ChunkManifest::chunk_file(seq),
            start_sample,
            len_samples: len,
        });
        if !report.repaired.contains(&seq) {
            report.adopted.push(seq);
            manifest.events.push(ManifestEvent {
                kind: ManifestEventKind::ChunkAdopted,
                at_ms: now_ms,
                seq: Some(seq),
                file: Some(ChunkManifest::chunk_file(seq)),
                samples: Some(len),
            });
        }
    }
    manifest.chunks.sort_by_key(|c| c.seq);
    // a repaired chunk shorter than its record: shrink the record and gap the remainder
    for record in &mut manifest.chunks {
        if report.repaired.contains(&record.seq) {
            let bytes = std::fs::read(track_dir.join(&record.file))?;
            if let Ok(decoded) = wav::decode(&bytes) {
                let actual = decoded.samples.len() as u64;
                if actual < record.len_samples {
                    manifest.gaps.push(Gap {
                        from_sample: record.start_sample + actual,
                        to_sample: record.start_sample + record.len_samples,
                        reason: GapReason::CaptureInterrupted,
                    });
                    record.len_samples = actual;
                }
            }
        }
    }
    manifest.gaps.sort_by_key(|g| g.from_sample);
    manifest.save(track_dir)?;
    report.manifest = Some(manifest);
    Ok(report)
}

/// The sample position of an adopted chunk: after every earlier record and gap. Sequence numbers
/// are never renumbered.
fn position_for(manifest: &ChunkManifest, seq: u32) -> u64 {
    manifest
        .chunks
        .iter()
        .filter(|c| c.seq < seq)
        .map(|c| c.start_sample + c.len_samples)
        .chain(manifest.gaps.iter().map(|g| g.to_sample))
        .max()
        .unwrap_or(0)
}
