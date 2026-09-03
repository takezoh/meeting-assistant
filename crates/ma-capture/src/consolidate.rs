//! encode → verify → rename → record → delete, per track. Any other order can lose audio. A track
//! has one origin, so it is encoded at exactly one rate; a format change is a successor track.

use crate::manifest::{ChunkManifest, ManifestEvent, ManifestEventKind};
use crate::wav;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashPoint {
    AfterEncode,
    AfterVerify,
    AfterRename,
    AfterRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsolidateError {
    Io(String),
    /// The encoder's output does not decode to the chunk sequence; chunks are kept.
    VerificationMismatch {
        expected_samples: u64,
        decoded_samples: u64,
    },
    /// A test-injected crash: the process died after `CrashPoint`.
    Crashed(CrashPoint),
    NoManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidateReport {
    pub flac_file: PathBuf,
    pub samples: u64,
    /// Whether this run encoded (a re-run after the record step encodes nothing).
    pub encoded: bool,
    pub chunks_deleted: usize,
}

/// The encoder seam (discretion-flac-encoder-binding). Tests inject a wrong one.
pub trait Encoder {
    fn encode(&self, sample_rate: u32, channels: u16, samples: &[i16]) -> Result<Vec<u8>, String>;
}

#[derive(Debug, Default)]
pub struct FlacEncoder;

impl Encoder for FlacEncoder {
    fn encode(&self, sample_rate: u32, channels: u16, samples: &[i16]) -> Result<Vec<u8>, String> {
        use flacenc::error::Verify;
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(|e| format!("{e:?}"))?;
        let pcm: Vec<i32> = samples.iter().map(|s| *s as i32).collect();
        let source = flacenc::source::MemSource::from_samples(
            &pcm,
            channels as usize,
            16,
            sample_rate as usize,
        );
        let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|e| format!("{e:?}"))?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        use flacenc::component::BitRepr;
        stream.write(&mut sink).map_err(|e| format!("{e:?}"))?;
        Ok(sink.as_slice().to_vec())
    }
}

pub fn decode_flac(bytes: &[u8]) -> Result<(u32, u16, Vec<i16>), String> {
    let mut reader =
        claxon::FlacReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let info = reader.streaminfo();
    let mut samples = Vec::new();
    for s in reader.samples() {
        samples.push(s.map_err(|e| e.to_string())? as i16);
    }
    Ok((info.sample_rate, info.channels as u16, samples))
}

/// The track's chunk sequence with its gaps rendered as silence, in track order.
pub fn expected_samples(
    track_dir: &Path,
    manifest: &ChunkManifest,
) -> Result<Vec<i16>, ConsolidateError> {
    let mut out: Vec<i16> = Vec::new();
    let mut spans: Vec<(u64, u64, Option<String>)> = manifest
        .chunks
        .iter()
        .map(|c| (c.start_sample, c.len_samples, Some(c.file.clone())))
        .chain(
            manifest
                .gaps
                .iter()
                .map(|g| (g.from_sample, g.to_sample - g.from_sample, None)),
        )
        .collect();
    spans.sort_by_key(|s| s.0);
    for (start, len, file) in spans {
        if (out.len() as u64) < start {
            out.resize(start as usize, 0);
        }
        match file {
            Some(file) => {
                let bytes = std::fs::read(track_dir.join(&file))
                    .map_err(|e| ConsolidateError::Io(format!("{file}: {e}")))?;
                let decoded = wav::decode(&bytes).map_err(ConsolidateError::Io)?;
                out.extend_from_slice(&decoded.samples);
            }
            None => out.resize(out.len() + len as usize, 0),
        }
    }
    Ok(out)
}

pub fn consolidate(
    track_dir: &Path,
    out_dir: &Path,
    now_ms: u64,
) -> Result<ConsolidateReport, ConsolidateError> {
    consolidate_with(track_dir, out_dir, &FlacEncoder, None, now_ms)
}

/// The full procedure with an injectable encoder and crash point.
pub fn consolidate_with(
    track_dir: &Path,
    out_dir: &Path,
    encoder: &dyn Encoder,
    crash_after: Option<CrashPoint>,
    now_ms: u64,
) -> Result<ConsolidateReport, ConsolidateError> {
    let mut manifest = ChunkManifest::load(track_dir)
        .map_err(ConsolidateError::Io)?
        .ok_or(ConsolidateError::NoManifest)?;
    std::fs::create_dir_all(out_dir).map_err(|e| ConsolidateError::Io(e.to_string()))?;
    let name = manifest.flac_name();
    let flac_file = out_dir.join(&name);
    let part = out_dir.join(format!("{name}.part"));
    // a leftover .part from a crash is discarded; the chunks are still there
    let _ = std::fs::remove_file(&part);
    let total_samples = manifest.end_sample();
    let mut encoded = false;
    if manifest.consolidated_file.is_none() || !flac_file.exists() {
        // chunks are still the archival form here, so they are all present
        let expected = expected_samples(track_dir, &manifest)?;
        let origin = manifest.origin.clone();
        let bytes = encoder
            .encode(origin.sample_rate, origin.channels, &expected)
            .map_err(ConsolidateError::Io)?;
        std::fs::write(&part, &bytes)
            .map_err(|e| ConsolidateError::Io(format!("{}: {e}", part.display())))?;
        encoded = true;
        if crash_after == Some(CrashPoint::AfterEncode) {
            return Err(ConsolidateError::Crashed(CrashPoint::AfterEncode));
        }
        // verify sample-exactly at the track's rate before anything else
        let (rate, channels, decoded) = decode_flac(&bytes).map_err(ConsolidateError::Io)?;
        if rate != origin.sample_rate || channels != origin.channels || decoded != expected {
            let _ = std::fs::remove_file(&part);
            manifest.events.push(ManifestEvent {
                kind: ManifestEventKind::ConsolidationFailed,
                at_ms: now_ms,
                seq: None,
                file: Some(name),
                samples: Some(decoded.len() as u64),
            });
            manifest
                .save(track_dir)
                .map_err(|e| ConsolidateError::Io(e.to_string()))?;
            return Err(ConsolidateError::VerificationMismatch {
                expected_samples: expected.len() as u64,
                decoded_samples: decoded.len() as u64,
            });
        }
        if crash_after == Some(CrashPoint::AfterVerify) {
            return Err(ConsolidateError::Crashed(CrashPoint::AfterVerify));
        }
        std::fs::rename(&part, &flac_file)
            .map_err(|e| ConsolidateError::Io(format!("rename {}: {e}", part.display())))?;
        if crash_after == Some(CrashPoint::AfterRename) {
            return Err(ConsolidateError::Crashed(CrashPoint::AfterRename));
        }
        manifest.consolidated_file = Some(name.clone());
        manifest.events.push(ManifestEvent {
            kind: ManifestEventKind::Consolidated,
            at_ms: now_ms,
            seq: None,
            file: Some(name),
            samples: Some(expected.len() as u64),
        });
        manifest
            .save(track_dir)
            .map_err(|e| ConsolidateError::Io(e.to_string()))?;
        if crash_after == Some(CrashPoint::AfterRecord) {
            return Err(ConsolidateError::Crashed(CrashPoint::AfterRecord));
        }
    }
    // verified and recorded: delete the chunks the manifest still lists
    let mut deleted = 0;
    for chunk in &manifest.chunks {
        let path = track_dir.join(&chunk.file);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| ConsolidateError::Io(e.to_string()))?;
            deleted += 1;
        }
    }
    if deleted > 0 {
        manifest.events.push(ManifestEvent {
            kind: ManifestEventKind::ChunksDeleted,
            at_ms: now_ms,
            seq: None,
            file: None,
            samples: Some(deleted as u64),
        });
        manifest
            .save(track_dir)
            .map_err(|e| ConsolidateError::Io(e.to_string()))?;
    }
    Ok(ConsolidateReport {
        flac_file,
        samples: total_samples,
        encoded,
        chunks_deleted: deleted,
    })
}
