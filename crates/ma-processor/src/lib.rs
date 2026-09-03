//! The processor seam (contract-processor-interface, contract-processor-budget,
//! contract-processing-isolation): a processor declares its capability and is refused outside it,
//! receives inputs as a staged directory holding exactly the declared files, is launched from an
//! argument vector built from a signed template with typed values substituted as whole arguments
//! (no shell, ever, and no secret in argv), reports monotonic progress per work item, observes
//! cancellation within a fixed bound, and treats a budget overrun as a warning. Every native or
//! external processor runs in `ma-processor-host`; `host.rs` fixes what crosses that boundary.

pub mod capability;
pub mod failure;
pub mod host;
pub mod progress;
pub mod scripted;
pub mod staging;

pub use capability::{Capability, ProcessorKind, ProcessorRequest, RunsIn};
pub use failure::{Failure, RetryCause, Warning};
pub use host::{
    build_argv, classify_exit, ArgvTemplate, ChildSpec, ExitOutcome, ParamType, ParamValue,
    ProgressFrame, RequestFrame, ResultFrame, SecretValue, StallWatch, HOST_MEMORY_CAP_BYTES,
    STALL_TIMEOUT_MS,
};
pub use progress::{
    run_items, CancellationToken, Clock, ItemOutcome, ItemRunner, ProgressTracker, RunReport,
    CANCELLATION_BOUND_MS, ITEM_BUDGET_MS,
};
pub use scripted::{Script, ScriptedProcessor};
pub use staging::{StagedDir, StagingError};

use serde::{Deserialize, Serialize};

/// Provenance carried by every output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub processor_id: String,
    pub version: String,
    pub model_id: String,
    pub model_digest: String,
    pub config_hash: String,
}

/// Verify a local model file against the digest pinned by the signed adapter manifest.
pub fn verify_model_digest(path: &std::path::Path, expected_sha256: &str) -> Result<(), Failure> {
    use sha2::Digest;
    let bytes = std::fs::read(path).map_err(|_| Failure::Permanent {
        reason: "model file unreadable".into(),
    })?;
    let actual = hex::encode(sha2::Sha256::digest(&bytes));
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(Failure::Permanent {
            reason: "model digest mismatch".into(),
        })
    }
}

/// The processor trait. Work is decomposed into items so cancellation and progress are bounded.
pub trait Processor {
    fn id(&self) -> &str;
    fn capability(&self) -> &Capability;
    /// Run one work item; `progress` is reported per item by the runner.
    fn run_item(&mut self, ordinal: u32, staged: &StagedDir) -> Result<Vec<u8>, Failure>;
}
