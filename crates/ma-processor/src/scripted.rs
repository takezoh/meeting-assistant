//! The scripted fake processor: slow, failing, cancel-ignoring, over-budget, quadratic, or
//! host-aborting on command. The contract tests and the host binary both use it.

use crate::capability::{Capability, ProcessorKind, RunsIn};
use crate::failure::{Failure, RetryCause};
use crate::progress::{CancellationToken, ItemOutcome, ItemRunner};
use crate::staging::StagedDir;
use crate::Processor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Script {
    /// Each item costs this many virtual milliseconds.
    ItemCostMs(u64),
    /// Item cost grows with the number of items before it (the quadratic anti-pattern).
    AccumulatingContext {
        base_ms: u64,
        per_prior_ms: u64,
    },
    FailRetryableAt(u32),
    /// Do not check the token: the item blocks for this long regardless.
    IgnoreCancellationFor(u64),
    /// Abort the host process at this item.
    AbortAt(u32),
    /// Emit no progress at all.
    Silent,
}

pub struct ScriptedProcessor {
    pub capability: Capability,
    pub scripts: Vec<Script>,
    pub virtual_now_ms: u64,
    pub items_done: u32,
}

impl ScriptedProcessor {
    pub fn transcription(languages: &[&str]) -> ScriptedProcessor {
        ScriptedProcessor {
            capability: Capability {
                kind: ProcessorKind::Transcription,
                languages: languages.iter().map(|l| l.to_string()).collect(),
                needs_gpu: false,
                max_input_seconds: 4 * 3600,
                streaming: false,
                egress_hosts: vec![],
                runs_in: RunsIn::Host,
            },
            scripts: vec![Script::ItemCostMs(100)],
            virtual_now_ms: 0,
            items_done: 0,
        }
    }
    pub fn with(mut self, script: Script) -> ScriptedProcessor {
        self.scripts.push(script);
        self
    }
    /// Virtual cost of item `ordinal`.
    pub fn cost_of(&self, ordinal: u32) -> u64 {
        let mut cost = 0;
        for s in &self.scripts {
            match s {
                Script::ItemCostMs(ms) => cost = *ms,
                Script::AccumulatingContext {
                    base_ms,
                    per_prior_ms,
                } => cost = base_ms + per_prior_ms * ordinal as u64,
                _ => {}
            }
        }
        cost
    }
}

impl Processor for ScriptedProcessor {
    fn id(&self) -> &str {
        "scripted"
    }
    fn capability(&self) -> &Capability {
        &self.capability
    }
    fn run_item(&mut self, ordinal: u32, _staged: &StagedDir) -> Result<Vec<u8>, Failure> {
        if self.scripts.contains(&Script::FailRetryableAt(ordinal)) {
            return Err(Failure::Retryable {
                after_ms: 1_000,
                cause: RetryCause::Transient,
            });
        }
        self.virtual_now_ms += self.cost_of(ordinal);
        self.items_done += 1;
        Ok(format!("item-{ordinal}").into_bytes())
    }
}

/// Runs the scripted processor against a virtual clock shared with the test.
pub struct ScriptedRunner<'a> {
    pub processor: &'a mut ScriptedProcessor,
    pub clock: &'a std::cell::Cell<u64>,
}

impl ItemRunner for ScriptedRunner<'_> {
    fn run(&mut self, ordinal: u32, cancel: &CancellationToken) -> ItemOutcome {
        for s in self.processor.scripts.clone() {
            match s {
                Script::IgnoreCancellationFor(ms) => {
                    // a blocking FFI call: the token is not consulted until it returns
                    self.clock.set(self.clock.get() + ms);
                    if cancel.is_cancelled() {
                        return ItemOutcome::Failed(Failure::Cancelled);
                    }
                }
                Script::FailRetryableAt(at) if at == ordinal => {
                    return ItemOutcome::Failed(Failure::Retryable {
                        after_ms: 1_000,
                        cause: RetryCause::Transient,
                    });
                }
                _ => {}
            }
        }
        if cancel.is_cancelled() {
            return ItemOutcome::Failed(Failure::Cancelled);
        }
        self.clock
            .set(self.clock.get() + self.processor.cost_of(ordinal));
        self.processor.items_done += 1;
        ItemOutcome::Done
    }
}
