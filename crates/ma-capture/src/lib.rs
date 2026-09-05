//! Durable audio (contract-chunk-durability, contract-session-timeline,
//! contract-track-consolidation). A `CaptureSource` per track becomes fixed-duration durable
//! chunks; the chunk directory is the truth and the manifest a cache; consolidation encodes, verifies
//! sample-exactly, renames, records and only then deletes. The Phase 0 `SyntheticSource` emits a
//! deterministic PCM ramp so all of it runs with no audio hardware.

pub mod chunk_writer;
pub mod consolidate;
pub mod manifest;
pub mod recovery;
pub mod source;
pub mod wasapi;
pub mod wav;

pub use chunk_writer::{
    is_disk_full, ChunkFs, ChunkWriter, DegradedReason, FsError, RealFs, WriterEvent,
    CHUNK_SAMPLES, QUEUE_CAP_SAMPLES, SAMPLE_RATE,
};
pub use consolidate::{
    consolidate, consolidate_with, decode_flac, expected_samples, ConsolidateError,
    ConsolidateReport, CrashPoint, Encoder, FlacEncoder,
};
pub use manifest::{
    ChunkManifest, ChunkRecord, ManifestEvent, ManifestEventKind, MANIFEST_FILE,
    MANIFEST_SCHEMA_VERSION,
};
pub use recovery::{recover, RecoveryReport};
pub use source::{CaptureSource, SourceEvent, SyntheticSource};
pub use wasapi::{
    ActivationBackend, ActivationError, ActivationOutcome, AudioStream, LoopbackTarget,
    StreamFormat, StreamRead, WasapiSource,
};
