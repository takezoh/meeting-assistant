//! Top-level error taxonomy. Internal contract violations fail fast; only externally caused
//! failures are retryable (design-quality error trichotomy).

use serde::{Deserialize, Serialize};

/// Whether a failure may be retried, and after how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Retryability {
    /// The cause is external and transient; retry after the given delay.
    Retryable { after_ms: u64 },
    /// The cause is external and will not change on retry.
    Permanent,
    /// The cause is an internal contract violation; a retry would hide a bug.
    Programming,
}

/// Errors raised by the core vocabulary itself.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("identifier `{0}` is not a valid UUID")]
    InvalidId(String),
    #[error("chunk at sample {start_sample} overlaps the previous chunk ending at sample {previous_end}")]
    ChunkOverlap {
        start_sample: u64,
        previous_end: u64,
    },
    #[error("gap [{from_sample}, {to_sample}) overlaps or is empty")]
    InvalidGap { from_sample: u64, to_sample: u64 },
    #[error("track range is not tiled: hole or overlap at sample {at_sample}")]
    TilingBroken { at_sample: u64 },
    #[error("sample rate must be positive")]
    ZeroSampleRate,
    #[error("artifact path segment `{0}` is not a generated identifier or typed name")]
    InvalidPathSegment(String),
    #[error("timestamp {sample} is beyond the covered range of the track")]
    SampleOutOfRange { sample: u64 },
}

impl CoreError {
    /// Every core error is a programming error: the vocabulary has no external cause.
    pub fn retryability(&self) -> Retryability {
        Retryability::Programming
    }
}
