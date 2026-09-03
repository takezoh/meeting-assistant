//! Intent before effect. A row is committed as `intended` before any effect outside the state
//! database, and updated to `committed` with the created resource's identity afterwards. A row
//! found `intended` on restart is the named outcome `unknown`, resolved by lookup or by an explicit
//! decision — never by a silent recreate.

use ma_core_types::id::TypedId;
use ma_core_types::{MeetingId, StepId};
use serde::{Deserialize, Serialize};

pub type IdempotencyKey = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Intended,
    Committed,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRow {
    pub effect_id: uuid::Uuid,
    pub meeting_id: MeetingId,
    pub step_id: StepId,
    pub kind: String,
    pub idempotency_key: IdempotencyKey,
    pub state: EffectState,
    pub resource_ref: Option<String>,
    pub at_ms: u64,
}

/// How the owning contract answers "did this effect happen?" on restart.
pub trait EffectLookup {
    fn find(&self, kind: &str, idempotency_key: &str) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectDecision {
    /// The user confirmed the effect did not happen (or does not matter); the step may redo it.
    Abandon,
    /// The user identified the created resource; the step reuses it.
    Adopt(String),
}

/// Handed to a step executor: the only way to perform an effect is through `intend` first.
pub struct EffectContext<'a> {
    pub(crate) store: &'a mut dyn crate::queue::WorkflowStore,
    pub(crate) step: &'a crate::step::Step,
    pub(crate) now_ms: u64,
    pub(crate) trace: &'a mut Vec<String>,
}

impl<'a> EffectContext<'a> {
    /// Construct a context outside the queue (tests that emulate a kill between effect and record).
    pub fn for_test(
        store: &'a mut dyn crate::queue::WorkflowStore,
        step: &'a crate::step::Step,
        now_ms: u64,
        trace: &'a mut Vec<String>,
    ) -> EffectContext<'a> {
        EffectContext {
            store,
            step,
            now_ms,
            trace,
        }
    }

    /// A previously committed effect for this key, so a re-run reuses instead of recreating.
    pub fn existing(&self, kind: &str, idempotency_key: &str) -> Option<String> {
        self.store
            .ledger_for_step(self.step.step_id)
            .into_iter()
            .find(|r| {
                r.kind == kind
                    && r.idempotency_key == idempotency_key
                    && r.state == EffectState::Committed
            })
            .and_then(|r| r.resource_ref)
    }

    /// Commit the `intended` row. Returns its id; the caller performs the effect and then `commit`s.
    pub fn intend(&mut self, kind: &str, idempotency_key: &str) -> uuid::Uuid {
        let row = EffectRow {
            effect_id: uuid::Uuid::new_v5(
                &self.step.step_id.uuid(),
                format!("{kind}:{idempotency_key}").as_bytes(),
            ),
            meeting_id: self.step.meeting_id,
            step_id: self.step.step_id,
            kind: kind.to_string(),
            idempotency_key: idempotency_key.to_string(),
            state: EffectState::Intended,
            resource_ref: None,
            at_ms: self.now_ms,
        };
        let id = row.effect_id;
        self.store.ledger_upsert(row);
        self.trace
            .push(format!("ledger:intended:{kind}:{idempotency_key}"));
        id
    }

    pub fn commit(&mut self, effect_id: uuid::Uuid, resource_ref: &str) {
        self.store.ledger_set_state(
            effect_id,
            EffectState::Committed,
            Some(resource_ref.to_string()),
        );
        self.trace.push(format!("ledger:committed:{resource_ref}"));
    }

    /// Record that an effect is about to happen in the trace, for tests that assert ordering.
    pub fn note_effect(&mut self, what: &str) {
        self.trace.push(format!("effect:{what}"));
    }
}
