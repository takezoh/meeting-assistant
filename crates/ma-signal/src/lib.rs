//! L1 contract crate: what a signal is (contract-signal-envelope), the collector seam and the
//! replayable timeline. Collectors observe; they never decide. There is no free-text UI field in
//! the envelope, so a DOM-, title- or coordinate-derived fact has nowhere to live.

pub mod adapter;
pub mod envelope;
pub mod source;
pub mod timeline;

pub use adapter::{
    conformance_violations, AdapterClass, AdapterFixtures, AdapterSpec, Corroboration, MatchKind,
    MeetingAdapter, TableAdapter,
};
pub use envelope::{
    Authority, ObservedAt, Payload, Signal, SignalKind, Subject, UserCommand, SCHEMA_VERSION,
};
pub use source::{FixtureSource, SignalSource, TimelineHeader};
pub use timeline::SignalTimeline;
