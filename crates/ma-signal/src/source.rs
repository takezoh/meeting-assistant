//! The collector seam and the fixture-replay source used by every Phase 0 detection test.

use crate::envelope::Signal;
use serde::{Deserialize, Serialize};

/// A stream of signals from one collector. Implemented by the Windows collectors, the extension
/// channel and by [`FixtureSource`] for replay.
pub trait SignalSource {
    fn source_id(&self) -> &str;
    fn next_signal(&mut self) -> Option<Signal>;
}

/// The first record of a `fixtures/signal-timelines/<name>.jsonl` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineHeader {
    pub schema_version: u32,
    pub adapter_table_version: u32,
    /// Redacted description of the recording machine.
    pub machine_profile: String,
    /// ISO-8601 date of recording.
    pub created: String,
}

/// Replays a recorded timeline in recorded order.
#[derive(Debug, Clone)]
pub struct FixtureSource {
    source_id: String,
    signals: std::vec::IntoIter<Signal>,
}

impl FixtureSource {
    pub fn new(source_id: impl Into<String>, signals: Vec<Signal>) -> Self {
        Self {
            source_id: source_id.into(),
            signals: signals.into_iter(),
        }
    }
}

impl SignalSource for FixtureSource {
    fn source_id(&self) -> &str {
        &self.source_id
    }
    fn next_signal(&mut self) -> Option<Signal> {
        self.signals.next()
    }
}
