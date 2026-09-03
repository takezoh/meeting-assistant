//! The adapter registry. The seam itself (`MeetingAdapter`, `TableAdapter`, conformance) is the L1
//! contract in `ma_signal::adapter` so that L4 adapter crates can implement it; this crate owns the
//! registry and isolates a panicking adapter instead of failing the pipeline.

use ma_signal::Subject;
use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub use ma_signal::adapter::{Corroboration, MatchKind, MeetingAdapter};

/// The registry populated by the composition root. Iteration order is registration order.
pub struct AdapterTable {
    version: u32,
    adapters: Vec<Box<dyn MeetingAdapter>>,
    disabled: BTreeSet<String>,
}

impl AdapterTable {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            adapters: Vec::new(),
            disabled: BTreeSet::new(),
        }
    }

    pub fn register(&mut self, adapter: Box<dyn MeetingAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn adapter(&self, id: &str) -> Option<&dyn MeetingAdapter> {
        self.adapters
            .iter()
            .find(|a| a.id() == id)
            .map(|a| a.as_ref())
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.contains(id)
    }

    /// Ask every enabled adapter whether it recognises the subject. A panicking adapter is
    /// reported, disabled for the remainder of the process, and treated as "did not match".
    pub fn matches(&mut self, subject: &Subject) -> (Vec<(String, MatchKind)>, Vec<String>) {
        let mut matched = Vec::new();
        let mut newly_disabled = Vec::new();
        for adapter in &self.adapters {
            let id = adapter.id().to_string();
            if self.disabled.contains(&id) {
                continue;
            }
            match catch_unwind(AssertUnwindSafe(|| adapter.matches(subject))) {
                Ok(Some(kind)) => matched.push((id, kind)),
                Ok(None) => {}
                Err(_) => {
                    self.disabled.insert(id.clone());
                    newly_disabled.push(id);
                }
            }
        }
        (matched, newly_disabled)
    }
}
