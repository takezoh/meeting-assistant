//! Monotonic progress with a trailing-window ETA, bounded cancellation, per-item cost tracking and
//! the budget-overrun warning.

use crate::failure::{Failure, Warning};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A work item's budget: its own media duration at the 1.0x target (30 s chunks).
pub const ITEM_BUDGET_MS: u64 = 30_000;
/// Cancellation must be observed within this bound.
pub const CANCELLATION_BOUND_MS: u64 = 5_000;
const ETA_WINDOW: usize = 8;

pub trait Clock {
    fn now_ms(&self) -> u64;
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Progress in completed work items over a total; never decreases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressTracker {
    pub total_items: u32,
    pub completed: u32,
    pub reports: Vec<(u64, u32)>,
    pub regressions_rejected: u32,
    item_durations_ms: Vec<u64>,
}

impl ProgressTracker {
    pub fn new(total_items: u32) -> ProgressTracker {
        ProgressTracker {
            total_items,
            completed: 0,
            reports: Vec::new(),
            regressions_rejected: 0,
            item_durations_ms: Vec::new(),
        }
    }

    /// Report completion of items so far. A lower value than already reported is rejected and counted.
    pub fn report(&mut self, completed: u32, now_ms: u64) -> u32 {
        let completed = completed.min(self.total_items);
        if completed < self.completed {
            self.regressions_rejected += 1;
        } else {
            self.completed = completed;
        }
        self.reports.push((now_ms, self.completed));
        self.completed
    }

    pub fn record_item_duration(&mut self, ms: u64) {
        self.item_durations_ms.push(ms);
    }

    pub fn item_durations_ms(&self) -> &[u64] {
        &self.item_durations_ms
    }

    /// ETA from observed throughput over the trailing window; `None` until there is a window.
    pub fn eta_ms(&self) -> Option<u64> {
        let n = self.item_durations_ms.len();
        if n == 0 || self.completed >= self.total_items {
            return if n == 0 { None } else { Some(0) };
        }
        let window = &self.item_durations_ms[n.saturating_sub(ETA_WINDOW)..];
        let per_item = window.iter().sum::<u64>() / window.len() as u64;
        Some(per_item * (self.total_items - self.completed) as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    Done,
    Failed(Failure),
}

/// One work item's execution; returns when the item is done or the processor noticed cancellation.
pub trait ItemRunner {
    fn run(&mut self, ordinal: u32, cancel: &CancellationToken) -> ItemOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub completed_items: u32,
    pub outcome: Result<(), Failure>,
    pub warnings: Vec<Warning>,
    pub progress: ProgressTracker,
    /// Wall time from cancel() to the runner returning, when cancellation happened.
    pub cancel_latency_ms: Option<u64>,
}

/// Drive the items in order, checking cancellation between items, tracking per-item cost and
/// emitting the budget warning without failing the step.
pub fn run_items(
    runner: &mut dyn ItemRunner,
    total_items: u32,
    cancel: &CancellationToken,
    clock: &dyn Clock,
    cancelled_at_ms: &dyn Fn() -> Option<u64>,
) -> RunReport {
    let mut progress = ProgressTracker::new(total_items);
    let started = clock.now_ms();
    let mut warnings = Vec::new();
    let mut outcome = Ok(());
    let mut cancel_latency_ms = None;
    for ordinal in 0..total_items {
        if cancel.is_cancelled() {
            outcome = Err(Failure::Cancelled);
            cancel_latency_ms = Some(
                clock
                    .now_ms()
                    .saturating_sub(cancelled_at_ms().unwrap_or(clock.now_ms())),
            );
            break;
        }
        let item_started = clock.now_ms();
        let result = runner.run(ordinal, cancel);
        progress.record_item_duration(clock.now_ms().saturating_sub(item_started));
        match result {
            ItemOutcome::Done => {
                progress.report(ordinal + 1, clock.now_ms());
            }
            ItemOutcome::Failed(failure) => {
                if failure == Failure::Cancelled {
                    cancel_latency_ms = Some(
                        clock
                            .now_ms()
                            .saturating_sub(cancelled_at_ms().unwrap_or(item_started)),
                    );
                }
                outcome = Err(failure);
                break;
            }
        }
    }
    let elapsed_ms = clock.now_ms().saturating_sub(started);
    let budget_ms = ITEM_BUDGET_MS * progress.completed as u64;
    if outcome.is_ok() && elapsed_ms > budget_ms {
        warnings.push(Warning::BudgetExceeded {
            budget_ms,
            elapsed_ms,
        });
    }
    RunReport {
        completed_items: progress.completed,
        outcome,
        warnings,
        progress,
        cancel_latency_ms,
    }
}
