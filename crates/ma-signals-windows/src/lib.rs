//! Windows signal collectors behind the [`ma_signal::SignalSource`] seam.
//!
//! Every collector in this crate is built over a small enumerator or observer trait that has a
//! live Windows implementation behind `cfg(windows)` and a portable fake used by the tests on every
//! host. The collectors emit only the closed `Signal` / `Subject` / `Payload` shapes `ma-signal`
//! declares; they add no field and no kind. Service identifiers (image names, package family
//! names) are constructor input supplied by the composition root from the `ma-adapter-*` tables —
//! this crate is L3 and must not carry a literal.

pub mod audio_session;
pub mod endpoint_observation;
pub mod mic_use;
pub mod package_identity;
pub mod process;

pub use audio_session::{
    FakeSessionManager, SessionEvent, SessionFlow, SessionManager, SessionManagerError,
    SessionState,
};
pub use endpoint_observation::{EndpointObservation, EndpointObservations};
pub use mic_use::{
    AudioSessionMicCollector, ConsentStore, ConsentUse, FakeConsentStore, MicCollectorDiagnostics,
    StartupFailure,
};
pub use package_identity::{PackageIdentity, PackageIdentityProbe};
pub use process::{
    CollectorDiagnostics, EnumerationError, FakeProcessEnumerator, ProcessEnumerator,
    ProcessPackageCollector, ProcessRecord, TargetApplications,
};

use ma_signal::ObservedAt;

/// The two clocks a collector stamps on every signal: a monotonic ordering clock and a wall clock
/// for display. Implemented by [`SystemClock`] and by the fixed fake the tests use.
pub trait Clock {
    fn now(&mut self) -> ObservedAt;
}

/// Reads the host's monotonic and wall clocks.
#[derive(Debug, Default)]
pub struct SystemClock {
    origin: Option<std::time::Instant>,
}

impl SystemClock {
    /// A clock whose monotonic zero is `origin`, so every collector, channel and track of one
    /// session shares one time base.
    pub fn with_origin(origin: std::time::Instant) -> Self {
        Self {
            origin: Some(origin),
        }
    }
}

impl Clock for SystemClock {
    fn now(&mut self) -> ObservedAt {
        let origin = *self.origin.get_or_insert_with(std::time::Instant::now);
        let monotonic_ns = origin.elapsed().as_nanos() as u64;
        let wall_utc_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        ObservedAt {
            monotonic_ns,
            wall_utc_ms,
        }
    }
}

/// A clock that advances by a fixed step on every read; deterministic for fixtures.
#[derive(Debug, Clone)]
pub struct SteppingClock {
    next_monotonic_ns: u64,
    step_ns: u64,
    wall_utc_ms: i64,
}

impl SteppingClock {
    pub fn new(start_monotonic_ns: u64, step_ns: u64, wall_utc_ms: i64) -> Self {
        Self {
            next_monotonic_ns: start_monotonic_ns,
            step_ns,
            wall_utc_ms,
        }
    }
}

impl Clock for SteppingClock {
    fn now(&mut self) -> ObservedAt {
        let at = ObservedAt {
            monotonic_ns: self.next_monotonic_ns,
            wall_utc_ms: self.wall_utc_ms + (self.next_monotonic_ns / 1_000_000) as i64,
        };
        self.next_monotonic_ns += self.step_ns;
        at
    }
}
