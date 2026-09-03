//! The durable queue over the `WorkflowStore` port: enqueue with idempotent keys, lease-based
//! claiming, recovery of expired leases and `intended` effects, and retry classification.

use crate::edits::{EditOverlay, Generation};
use crate::effect_ledger::{EffectContext, EffectDecision, EffectLookup, EffectRow, EffectState};
use crate::lifecycle::ArtifactRecord;
use crate::retry::{backoff_ms, RetryClass};
use crate::step::{Step, StepKey, StepSpec, StepStatus, WorkItem, WorkItemStatus};
use ma_core_types::id::TypedId;
use ma_core_types::{MeetingId, StepId};
use std::collections::BTreeMap;

pub const DEFAULT_LEASE_MS: u64 = 60_000;

/// The persistence port. The engine implements it over `ma-store`; tests use `MemoryStore`.
pub trait WorkflowStore {
    fn step_by_key(&self, key: &StepKey) -> Option<Step>;
    fn step(&self, id: StepId) -> Option<Step>;
    fn steps(&self) -> Vec<Step>;
    fn insert_step(&mut self, step: Step, items: Vec<WorkItem>);
    fn update_step(&mut self, step: Step);
    fn work_items(&self, step_id: StepId) -> Vec<WorkItem>;
    fn update_work_item(&mut self, item: WorkItem);
    fn ledger_for_step(&self, step_id: StepId) -> Vec<EffectRow>;
    fn ledger_for_meeting(&self, meeting_id: MeetingId) -> Vec<EffectRow>;
    fn ledger_upsert(&mut self, row: EffectRow);
    fn ledger_set_state(
        &mut self,
        effect_id: uuid::Uuid,
        state: EffectState,
        resource_ref: Option<String>,
    );
    fn artifacts(&self, meeting_id: MeetingId) -> Vec<ArtifactRecord>;
    fn insert_artifact(&mut self, artifact: ArtifactRecord);
    fn generations(&self, meeting_id: MeetingId) -> Vec<Generation>;
    fn insert_generation(&mut self, generation: Generation);
    fn overlays(&self, meeting_id: MeetingId) -> Vec<EditOverlay>;
    fn insert_overlay(&mut self, overlay: EditOverlay);
    fn update_overlay(&mut self, overlay: EditOverlay);
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    steps: BTreeMap<String, Step>,
    items: BTreeMap<String, Vec<WorkItem>>,
    ledger: BTreeMap<uuid::Uuid, EffectRow>,
    artifacts: Vec<ArtifactRecord>,
    generations: Vec<Generation>,
    overlays: BTreeMap<uuid::Uuid, EditOverlay>,
}

impl WorkflowStore for MemoryStore {
    fn step_by_key(&self, key: &StepKey) -> Option<Step> {
        self.steps.values().find(|s| &s.key == key).cloned()
    }
    fn step(&self, id: StepId) -> Option<Step> {
        self.steps.get(&id.to_string()).cloned()
    }
    fn steps(&self) -> Vec<Step> {
        self.steps.values().cloned().collect()
    }
    fn insert_step(&mut self, step: Step, items: Vec<WorkItem>) {
        self.items.insert(step.step_id.to_string(), items);
        self.steps.insert(step.step_id.to_string(), step);
    }
    fn update_step(&mut self, step: Step) {
        self.steps.insert(step.step_id.to_string(), step);
    }
    fn work_items(&self, step_id: StepId) -> Vec<WorkItem> {
        self.items
            .get(&step_id.to_string())
            .cloned()
            .unwrap_or_default()
    }
    fn update_work_item(&mut self, item: WorkItem) {
        if let Some(items) = self.items.get_mut(&item.step_id.to_string()) {
            if let Some(slot) = items.iter_mut().find(|i| i.ordinal == item.ordinal) {
                *slot = item;
            }
        }
    }
    fn ledger_for_step(&self, step_id: StepId) -> Vec<EffectRow> {
        self.ledger
            .values()
            .filter(|r| r.step_id == step_id)
            .cloned()
            .collect()
    }
    fn ledger_for_meeting(&self, meeting_id: MeetingId) -> Vec<EffectRow> {
        self.ledger
            .values()
            .filter(|r| r.meeting_id == meeting_id)
            .cloned()
            .collect()
    }
    fn ledger_upsert(&mut self, row: EffectRow) {
        self.ledger.insert(row.effect_id, row);
    }
    fn ledger_set_state(
        &mut self,
        effect_id: uuid::Uuid,
        state: EffectState,
        resource_ref: Option<String>,
    ) {
        if let Some(row) = self.ledger.get_mut(&effect_id) {
            row.state = state;
            if resource_ref.is_some() {
                row.resource_ref = resource_ref;
            }
        }
    }
    fn artifacts(&self, meeting_id: MeetingId) -> Vec<ArtifactRecord> {
        self.artifacts
            .iter()
            .filter(|a| a.meeting_id == meeting_id)
            .cloned()
            .collect()
    }
    fn insert_artifact(&mut self, artifact: ArtifactRecord) {
        self.artifacts.push(artifact);
    }
    fn generations(&self, meeting_id: MeetingId) -> Vec<Generation> {
        self.generations
            .iter()
            .filter(|g| g.meeting_id == meeting_id)
            .cloned()
            .collect()
    }
    fn insert_generation(&mut self, generation: Generation) {
        self.generations.push(generation);
    }
    fn overlays(&self, meeting_id: MeetingId) -> Vec<EditOverlay> {
        self.overlays
            .values()
            .filter(|o| o.meeting_id == meeting_id)
            .cloned()
            .collect()
    }
    fn insert_overlay(&mut self, overlay: EditOverlay) {
        self.overlays.insert(overlay.overlay_id, overlay);
    }
    fn update_overlay(&mut self, overlay: EditOverlay) {
        self.overlays.insert(overlay.overlay_id, overlay);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Enqueued(StepId),
    /// The key already succeeded: nothing executes; the recorded result is returned.
    AlreadySucceeded {
        step_id: StepId,
        result_ref: Option<String>,
    },
    /// The key is already queued or running.
    AlreadyQueued(StepId),
    /// The key ended in a terminal failure or was cancelled: nothing will run; the caller decides.
    Terminal {
        step_id: StepId,
        status: StepStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub result_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepError {
    pub class: RetryClass,
    pub message: String,
}

/// A processor or destination invocation, as the queue sees it. Effects go through the context.
pub trait StepExecutor {
    fn execute(
        &mut self,
        step: &Step,
        ctx: &mut EffectContext<'_>,
    ) -> Result<StepResult, StepError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub step_id: StepId,
    pub status: StepStatus,
    pub executed: bool,
    /// Ordered trace of ledger writes and effects, for ordering assertions.
    pub trace: Vec<String>,
}

pub struct Queue<S: WorkflowStore> {
    pub store: S,
    pub lease_ms: u64,
}

impl<S: WorkflowStore> Queue<S> {
    pub fn new(store: S) -> Queue<S> {
        Queue {
            store,
            lease_ms: DEFAULT_LEASE_MS,
        }
    }

    /// Idempotent enqueue: a succeeded key returns its result and executes nothing.
    pub fn enqueue(&mut self, spec: &StepSpec) -> EnqueueOutcome {
        let key = spec.key();
        if let Some(existing) = self.store.step_by_key(&key) {
            return match existing.status {
                StepStatus::Succeeded => EnqueueOutcome::AlreadySucceeded {
                    step_id: existing.step_id,
                    result_ref: existing.result_ref,
                },
                StepStatus::FailedPermanent { .. } | StepStatus::Cancelled => {
                    EnqueueOutcome::Terminal {
                        step_id: existing.step_id,
                        status: existing.status,
                    }
                }
                _ => EnqueueOutcome::AlreadyQueued(existing.step_id),
            };
        }
        let step_id = StepId::new();
        let items = (0..spec.work_items.max(1))
            .map(|ordinal| WorkItem {
                work_item_id: WorkItem::stable_id(step_id, ordinal),
                step_id,
                ordinal,
                status: WorkItemStatus::Pending,
            })
            .collect();
        let step = Step {
            step_id,
            meeting_id: spec.meeting_id,
            session_id: spec.session_id,
            key,
            kind: spec.kind,
            processor_id: spec.processor_id.clone(),
            processor_version: spec.processor_version.clone(),
            config_hash: spec.config_hash.clone(),
            status: StepStatus::Pending,
            result_ref: None,
            attempts: 0,
        };
        self.store.insert_step(step, items);
        EnqueueOutcome::Enqueued(step_id)
    }

    /// Claim the next runnable step under a lease.
    pub fn claim(&mut self, now_ms: u64) -> Option<Step> {
        let mut candidates: Vec<Step> = self
            .store
            .steps()
            .into_iter()
            .filter(|s| match &s.status {
                StepStatus::Pending => true,
                StepStatus::FailedRetryable { not_before_ms, .. } => *not_before_ms <= now_ms,
                _ => false,
            })
            .collect();
        candidates.sort_by_key(|s| s.step_id);
        let mut step = candidates.into_iter().next()?;
        step.status = StepStatus::Running {
            lease_until_ms: now_ms + self.lease_ms,
        };
        step.attempts += 1;
        self.store.update_step(step.clone());
        Some(step)
    }

    /// Run one claimed step through the executor, applying retry classification.
    pub fn run(&mut self, step: Step, now_ms: u64, executor: &mut dyn StepExecutor) -> RunReport {
        let mut trace = Vec::new();
        let outcome = {
            let mut ctx = EffectContext {
                store: &mut self.store,
                step: &step,
                now_ms,
                trace: &mut trace,
            };
            executor.execute(&step, &mut ctx)
        };
        let mut step = self.store.step(step.step_id).expect("claimed step exists");
        match outcome {
            Ok(result) => {
                step.status = StepStatus::Succeeded;
                step.result_ref = Some(result.result_ref);
                for mut item in self.store.work_items(step.step_id) {
                    item.status = WorkItemStatus::Done;
                    self.store.update_work_item(item);
                }
            }
            Err(err) => {
                let unknown_effect = self
                    .store
                    .ledger_for_step(step.step_id)
                    .iter()
                    .any(|r| r.state == EffectState::Intended);
                step.status = if unknown_effect {
                    // the effect may or may not have happened: never re-run blindly; recover() resolves it
                    StepStatus::AwaitingDecision
                } else {
                    match (err.class, backoff_ms(step.attempts)) {
                        (RetryClass::Retryable, Some(delay)) => StepStatus::FailedRetryable {
                            attempts: step.attempts,
                            last_error: err.message,
                            not_before_ms: now_ms + delay,
                        },
                        _ => StepStatus::FailedPermanent {
                            attempts: step.attempts,
                            last_error: err.message,
                        },
                    }
                };
            }
        }
        self.store.update_step(step.clone());
        RunReport {
            step_id: step.step_id,
            status: step.status,
            executed: true,
            trace,
        }
    }

    /// Startup recovery: expired leases return to `pending`; `intended` effects are resolved by
    /// lookup, or the step waits for an explicit decision. Nothing is recreated silently.
    pub fn recover(&mut self, now_ms: u64, lookup: &dyn EffectLookup) -> Vec<StepId> {
        let mut runnable = Vec::new();
        for mut step in self.store.steps() {
            let expired = matches!(step.status, StepStatus::Running { lease_until_ms } if lease_until_ms <= now_ms);
            if !expired && step.status != StepStatus::AwaitingDecision {
                continue;
            }
            let mut unresolved = false;
            for row in self.store.ledger_for_step(step.step_id) {
                if row.state != EffectState::Intended {
                    continue;
                }
                match lookup.find(&row.kind, &row.idempotency_key) {
                    Some(resource_ref) => self.store.ledger_set_state(
                        row.effect_id,
                        EffectState::Committed,
                        Some(resource_ref),
                    ),
                    None => unresolved = true,
                }
            }
            step.status = if unresolved {
                StepStatus::AwaitingDecision
            } else {
                StepStatus::Pending
            };
            if !unresolved {
                runnable.push(step.step_id);
            }
            self.store.update_step(step);
        }
        runnable
    }

    /// The user's answer for an effect left `intended`; the step returns to `pending`.
    pub fn decide(&mut self, effect_id: uuid::Uuid, decision: EffectDecision) {
        let row = self
            .store
            .steps()
            .into_iter()
            .flat_map(|s| self.store.ledger_for_step(s.step_id))
            .find(|r| r.effect_id == effect_id);
        let Some(row) = row else { return };
        match decision {
            EffectDecision::Abandon => {
                self.store
                    .ledger_set_state(effect_id, EffectState::Abandoned, None)
            }
            EffectDecision::Adopt(resource_ref) => {
                self.store
                    .ledger_set_state(effect_id, EffectState::Committed, Some(resource_ref))
            }
        }
        if let Some(mut step) = self.store.step(row.step_id) {
            if step.status == StepStatus::AwaitingDecision
                && !self
                    .store
                    .ledger_for_step(step.step_id)
                    .iter()
                    .any(|r| r.state == EffectState::Intended)
            {
                step.status = StepStatus::Pending;
                self.store.update_step(step);
            }
        }
    }
}
