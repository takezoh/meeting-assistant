//! The capture endpoint a meeting application's microphone session is bound to, observed by the
//! audio-session collector and exposed as capture-side data (contract-audio-session-mic-use,
//! adr-20260904-mic-endpoint-observed-outside-the-signal-envelope).
//!
//! This is deliberately *not* a signal: `Subject` is a closed union and a `MicCaptureStarted`
//! attributed to a process cannot also name a device. The composition root reads the observation
//! through this accessor and hands the endpoint identifier to `ma-capture` as a plain string.

use std::collections::BTreeMap;

/// One process's currently active microphone endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointObservation {
    pub pid: u32,
    /// The MMDevice endpoint identifier string.
    pub endpoint_id: String,
    /// The session instance the observation came from.
    pub session_key: String,
    /// Monotonic time the capture session became active.
    pub since_monotonic_ns: u64,
}

/// The current endpoint per process, kept by the collector.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EndpointObservations {
    by_pid: BTreeMap<u32, EndpointObservation>,
}

impl EndpointObservations {
    pub fn record(&mut self, observation: EndpointObservation) {
        self.by_pid.insert(observation.pid, observation);
    }

    /// Clears the observation when the session that produced it is no longer active.
    pub fn clear(&mut self, pid: u32, session_key: &str) {
        if self
            .by_pid
            .get(&pid)
            .is_some_and(|o| o.session_key == session_key)
        {
            self.by_pid.remove(&pid);
        }
    }

    /// The endpoint identifier the process is capturing on, if any.
    pub fn endpoint_for(&self, pid: u32) -> Option<&str> {
        self.by_pid.get(&pid).map(|o| o.endpoint_id.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = &EndpointObservation> {
        self.by_pid.values()
    }

    pub fn is_empty(&self) -> bool {
        self.by_pid.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_only_removes_the_session_that_recorded_it() {
        let mut obs = EndpointObservations::default();
        obs.record(EndpointObservation {
            pid: 7,
            endpoint_id: "{ep-a}".into(),
            session_key: "s1".into(),
            since_monotonic_ns: 1,
        });
        obs.clear(7, "other");
        assert_eq!(obs.endpoint_for(7), Some("{ep-a}"));
        obs.clear(7, "s1");
        assert_eq!(obs.endpoint_for(7), None);
        assert!(obs.is_empty());
    }
}
