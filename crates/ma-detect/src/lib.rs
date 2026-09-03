//! L2 domain crate: `decide(&SignalTimeline, &DetectorConfig, &AdapterTable) -> DetectorOutput`
//! is pure and replayable (contract-detector-determinism) and lands every evaluation in exactly
//! one of four outcomes (contract-detector-outcome-partition). This crate contains no service
//! name; adapters are registered by the composition root only.
//!
//! Purity is enforced mechanically: `boundary.toml` forbids `std::time`, `std::fs`, `std::net`,
//! `std::process`, `rand` and `std::collections::HashMap` in this crate.

pub mod adapter;
pub mod decision;
pub mod detector;
pub mod outcome;

pub use adapter::{AdapterTable, Corroboration, MatchKind, MeetingAdapter};
pub use decision::{Decision, DetectorOutput, Diagnostic};
pub use detector::{decide, DetectorConfig};
pub use outcome::{partition, Outcome, Phase, SuppressionReason};
