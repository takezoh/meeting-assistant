//! L0 kernel crate: the vocabulary every other crate shares.
//!
//! - [`id`]: UUIDv7 identifiers that are reproduced verbatim across database rows, filesystem
//!   path segments and export payloads (contract-stable-identity).
//! - [`timeline`]: sample-domain track origins, chunk spans and first-class gaps
//!   (contract-session-timeline).
//! - [`artifact_ref`]: root-relative artifact addressing composed only of generated identifiers
//!   (contract-artifact-addressing).
//! - [`error`]: the top-level error taxonomy shared by the workflow, processor and store crates.
//!
//! The crate is pure: no platform, I/O or clock dependency beyond identifier minting.

pub mod artifact_ref;
pub mod error;
pub mod id;
pub mod timeline;

pub use artifact_ref::{ArtifactKind, ArtifactRef, PathSegment};
pub use error::{CoreError, Retryability};
pub use id::{
    ArtifactId, ChunkId, ChunkSeq, DecisionId, ExportId, MeetingId, RootId, SessionId, SignalId,
    StepId, TrackId,
};
pub use timeline::{
    CaptureMode, ChunkSpan, ContaminationRisk, Gap, GapReason, SessionTime, SessionTimeline,
    TrackOrigin, TrackSegment,
};
