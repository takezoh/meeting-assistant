//! Artifact lifecycle and the workflow side of meeting deletion: cancel every in-flight step, and
//! report `intended` ledger rows that must be resolved before a purge may run.

use crate::effect_ledger::EffectState;
use crate::queue::WorkflowStore;
use crate::step::StepStatus;
use ma_core_types::{ArtifactId, MeetingId, StepId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    Staged,
    Committed,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: ArtifactId,
    pub meeting_id: MeetingId,
    pub kind: String,
    pub relative_path: String,
    pub generation_id: Option<uuid::Uuid>,
    pub state: ArtifactState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurgeReadiness {
    Ready,
    /// These effects are `intended` with no outcome; the purge must not race them.
    BlockedOnIntended(Vec<uuid::Uuid>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelReport {
    pub cancelled: Vec<StepId>,
    /// Steps that were running: the host child for each must be killed by the supervisor.
    pub kill_requests: Vec<StepId>,
    pub readiness: PurgeReadiness,
}

/// Phase 1 of `meeting.delete` on the workflow side. A request, not an assumption.
pub fn cancel_for_meeting(store: &mut dyn WorkflowStore, meeting_id: MeetingId) -> CancelReport {
    let mut cancelled = Vec::new();
    let mut kill_requests = Vec::new();
    for mut step in store
        .steps()
        .into_iter()
        .filter(|s| s.meeting_id == meeting_id)
    {
        match step.status {
            StepStatus::Pending
            | StepStatus::FailedRetryable { .. }
            | StepStatus::AwaitingDecision => {
                step.status = StepStatus::Cancelled;
                cancelled.push(step.step_id);
                store.update_step(step);
            }
            StepStatus::Running { .. } => {
                step.status = StepStatus::Cancelled;
                cancelled.push(step.step_id);
                kill_requests.push(step.step_id);
                store.update_step(step);
            }
            StepStatus::Succeeded | StepStatus::FailedPermanent { .. } | StepStatus::Cancelled => {}
        }
    }
    let intended: Vec<uuid::Uuid> = store
        .ledger_for_meeting(meeting_id)
        .into_iter()
        .filter(|r| r.state == EffectState::Intended)
        .map(|r| r.effect_id)
        .collect();
    let readiness = if intended.is_empty() {
        PurgeReadiness::Ready
    } else {
        PurgeReadiness::BlockedOnIntended(intended)
    };
    CancelReport {
        cancelled,
        kill_requests,
        readiness,
    }
}
