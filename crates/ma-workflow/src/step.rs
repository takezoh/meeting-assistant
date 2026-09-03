//! Step identity: the key is computed once at enqueue from everything that would change the output.

use ma_core_types::{ArtifactId, MeetingId, SessionId, StepId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Consolidate,
    Transcribe,
    Diarize,
    Summarize,
    Export,
}

/// `hash(session_id, step_kind, ordered input artifact ids, processor_id, processor_version, config_hash)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StepKey(pub String);

impl StepKey {
    pub fn compute(
        session_id: SessionId,
        kind: StepKind,
        inputs: &[ArtifactId],
        processor_id: &str,
        processor_version: &str,
        config_hash: &str,
    ) -> StepKey {
        let mut hasher = Sha256::new();
        hasher.update(session_id.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(
            serde_json::to_string(&kind)
                .expect("kind serializes")
                .as_bytes(),
        );
        hasher.update([0]);
        for input in inputs {
            hasher.update(input.to_string().as_bytes());
            hasher.update([1]);
        }
        hasher.update([0]);
        hasher.update(processor_id.as_bytes());
        hasher.update([0]);
        hasher.update(processor_version.as_bytes());
        hasher.update([0]);
        hasher.update(config_hash.as_bytes());
        StepKey(hex::encode(hasher.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running {
        lease_until_ms: u64,
    },
    Succeeded,
    FailedRetryable {
        attempts: u32,
        last_error: String,
        not_before_ms: u64,
    },
    FailedPermanent {
        attempts: u32,
        last_error: String,
    },
    Cancelled,
    /// An effect of this step is `intended` with no committed outcome and no lookup could decide it.
    AwaitingDecision,
}

/// Everything the caller supplies at enqueue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepSpec {
    pub meeting_id: MeetingId,
    pub session_id: SessionId,
    pub kind: StepKind,
    pub inputs: Vec<ArtifactId>,
    pub processor_id: String,
    pub processor_version: String,
    pub config_hash: String,
    /// Per-chunk work items for transcription; one item otherwise.
    pub work_items: u32,
}

impl StepSpec {
    pub fn key(&self) -> StepKey {
        StepKey::compute(
            self.session_id,
            self.kind,
            &self.inputs,
            &self.processor_id,
            &self.processor_version,
            &self.config_hash,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub step_id: StepId,
    pub meeting_id: MeetingId,
    pub session_id: SessionId,
    pub key: StepKey,
    pub kind: StepKind,
    pub processor_id: String,
    pub processor_version: String,
    pub config_hash: String,
    pub status: StepStatus,
    pub result_ref: Option<String>,
    pub attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Pending,
    Done,
    Failed,
}

/// A per-chunk unit of a step with a stable id derived from the step id and ordinal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub work_item_id: uuid::Uuid,
    pub step_id: StepId,
    pub ordinal: u32,
    pub status: WorkItemStatus,
}

impl WorkItem {
    pub fn stable_id(step_id: StepId, ordinal: u32) -> uuid::Uuid {
        use ma_core_types::id::TypedId;
        uuid::Uuid::new_v5(&step_id.uuid(), format!("work-item:{ordinal}").as_bytes())
    }
}
