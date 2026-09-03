//! The workflow core (contract-workflow-step-idempotency, contract-retention-purge): step identity
//! and idempotency, the durable queue with leases, retry classification, the effect ledger's
//! intent-before-effect procedure, artifact lifecycle, and the separation of generated content from
//! user edits. This crate is layer L2: persistence reaches it through the `WorkflowStore` port,
//! which the engine implements over `ma-store` and tests implement in memory. No processor or
//! destination lives here.

pub mod edits;
pub mod effect_ledger;
pub mod lifecycle;
pub mod queue;
pub mod retry;
pub mod step;

pub use edits::{
    compose, propose_edit, reanchor, text_hash, Anchor, AnchorBasis, ComposedView, EditError,
    EditOverlay, Generation, TargetKind,
};
pub use effect_ledger::{
    EffectContext, EffectDecision, EffectLookup, EffectRow, EffectState, IdempotencyKey,
};
pub use lifecycle::{
    cancel_for_meeting, ArtifactRecord, ArtifactState, CancelReport, PurgeReadiness,
};
pub use queue::{
    EnqueueOutcome, MemoryStore, Queue, RunReport, StepError, StepExecutor, StepResult,
    WorkflowStore, DEFAULT_LEASE_MS,
};
pub use retry::{backoff_ms, RetryClass, BACKOFF_SCHEDULE_MS, MAX_ATTEMPTS};
pub use step::{Step, StepKey, StepKind, StepSpec, StepStatus, WorkItem, WorkItemStatus};
