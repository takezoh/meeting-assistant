//! The per-track chunk manifest: a cache of what the chunk directory holds, rewritten atomically
//! and fsynced after every durable step. `contracts/artifact/chunk-manifest.schema.json` is its shape.
//! The directory and file names use the track's identifier (contract-artifact-addressing); the
//! human role of the track (`mic`, `loopback`) is data inside the manifest. A track has exactly one
//! origin: a device format or endpoint change opens a *successor track* with its own identifier,
//! directory, origin and sample space (contract-session-timeline, `TrackSegment::open_successor`),
//! so every consolidated file is `tracks/<track_id>.flac` and every origin has a track row.

use ma_core_types::timeline::{Gap, TrackOrigin};
use ma_core_types::TrackId;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub seq: u32,
    pub file: String,
    pub start_sample: u64,
    pub len_samples: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestEventKind {
    ChunkRepaired,
    ChunkDiscarded,
    ChunkAdopted,
    ManifestRebuilt,
    SuccessorOpened,
    Consolidated,
    ChunksDeleted,
    ConsolidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEvent {
    pub kind: ManifestEventKind,
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkManifest {
    pub schema_version: u32,
    /// The track identifier; also the chunk directory name and the FLAC file stem.
    pub track: TrackId,
    /// The human role of the track (`mic`, `loopback`), never used in a path.
    pub role: String,
    pub origin: TrackOrigin,
    /// The track this one continues after a device format or endpoint change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<TrackId>,
    /// The track opened when this one ended on a format or endpoint change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor: Option<TrackId>,
    pub chunks: Vec<ChunkRecord>,
    pub gaps: Vec<Gap>,
    pub events: Vec<ManifestEvent>,
    /// Set once `<track_id>.flac` is verified and recorded; chunks may then be deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consolidated_file: Option<String>,
}

impl ChunkManifest {
    pub fn new(track: TrackId, role: &str, origin: TrackOrigin) -> ChunkManifest {
        ChunkManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            track,
            role: role.to_string(),
            origin,
            predecessor: None,
            successor: None,
            chunks: Vec::new(),
            gaps: Vec::new(),
            events: Vec::new(),
            consolidated_file: None,
        }
    }

    /// `Ok(None)` when there is no manifest; `Err` when one exists but cannot be read, so a corrupt
    /// cache is reported rather than silently treated as absent.
    pub fn load(track_dir: &Path) -> Result<Option<ChunkManifest>, String> {
        let path = track_dir.join(MANIFEST_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("manifest unreadable: {e}"))
    }

    /// Atomic rewrite: temp file, fsync, rename, fsync directory.
    pub fn save(&self, track_dir: &Path) -> std::io::Result<()> {
        let tmp = track_dir.join("manifest.json.tmp");
        let bytes = serde_json::to_vec_pretty(self).expect("manifest serializes");
        {
            let mut f = std::fs::File::create(&tmp)?;
            use std::io::Write;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, track_dir.join(MANIFEST_FILE))?;
        if let Ok(dir) = std::fs::File::open(track_dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    pub fn chunk_file(seq: u32) -> String {
        format!("{seq:06}.wav")
    }

    pub fn max_seq(&self) -> Option<u32> {
        self.chunks.iter().map(|c| c.seq).max()
    }

    /// The consolidated file name: `<track_id>.flac`, the store's `ArtifactLayout::track_flac`.
    pub fn flac_name(&self) -> String {
        format!("{}.flac", self.track)
    }

    /// One past the last sample this track covers (chunks and gaps).
    pub fn end_sample(&self) -> u64 {
        self.chunks
            .iter()
            .map(|c| c.start_sample + c.len_samples)
            .chain(self.gaps.iter().map(|g| g.to_sample))
            .max()
            .unwrap_or(0)
    }
}
