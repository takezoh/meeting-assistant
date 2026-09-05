//! The microphone the meeting application is using (contract-mic-endpoint-follows-session).
//!
//! The endpoint identifier arrives as a plain `Option<&str>` from the composition root, which reads
//! it from the audio-session collector's observation accessor; `ma-capture` names no type from
//! `ma-signals-windows`. `None` means the system default capture device, and that choice is
//! recorded. A changed hint is re-evaluated through the existing successor-track mechanism: the
//! source reopens on the new endpoint and surfaces `SourceEvent::FormatChanged` with the new
//! origin, which the durability path turns into `TrackSegment::open_successor`.
//!
//! Discretion `discretion-mic-endpoint-matching-heuristic`: hints coalesce. When several hints
//! arrive between two reads, only the latest one is authoritative and a single `FormatChanged`
//! results; an intermediate endpoint is never opened.

use super::{ActivationBackend, ActivationError, OriginClock, WasapiSource};
use crate::source::{CaptureSource, SourceEvent};
use ma_core_types::timeline::TrackOrigin;

/// Which endpoint the microphone source is open on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointChoice {
    /// The endpoint the meeting application's own session is bound to.
    Supplied(String),
    /// No hint was available; the system default capture device was opened and that fact recorded.
    SystemDefault,
}

impl EndpointChoice {
    fn from_hint(hint: Option<&str>) -> Self {
        match hint {
            Some(id) if !id.is_empty() => EndpointChoice::Supplied(id.to_string()),
            _ => EndpointChoice::SystemDefault,
        }
    }
    pub fn endpoint_id(&self) -> Option<&str> {
        match self {
            EndpointChoice::Supplied(id) => Some(id),
            EndpointChoice::SystemDefault => None,
        }
    }
}

/// The selection record kept next to the track: what was requested, what was opened, and how the
/// selection evolved. `TrackOrigin` carries no endpoint, so this is where the endpoint is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSelection {
    pub opened: EndpointChoice,
    /// Every endpoint choice opened so far, oldest first; one entry per track segment.
    pub history: Vec<EndpointChoice>,
    /// Hints that arrived and were superseded before the next read (coalesced, never opened).
    pub coalesced_hints: u32,
    /// Endpoint switches that failed to open; the current endpoint stays open in that case.
    pub failed_switches: u32,
}

/// A microphone `CaptureSource` that follows the supplied endpoint hint.
pub struct MicEndpointSource<B: ActivationBackend> {
    inner: WasapiSource<B>,
    selection: EndpointSelection,
    pending: Option<EndpointChoice>,
}

impl<B: ActivationBackend> MicEndpointSource<B> {
    /// Opens the microphone on `preferred_endpoint_id`, or on the system default when `None`.
    pub fn open(
        backend: B,
        preferred_endpoint_id: Option<&str>,
        clock: OriginClock,
    ) -> Result<Self, ActivationError> {
        let choice = EndpointChoice::from_hint(preferred_endpoint_id);
        let inner = WasapiSource::open_manual_device(backend, choice.endpoint_id(), clock)?;
        Ok(Self {
            inner,
            selection: EndpointSelection {
                opened: choice.clone(),
                history: vec![choice],
                coalesced_hints: 0,
                failed_switches: 0,
            },
            pending: None,
        })
    }

    pub fn selection(&self) -> &EndpointSelection {
        &self.selection
    }

    /// The activation backend, so a caller or test can observe which endpoint was requested.
    pub fn backend(&self) -> &B {
        self.inner.backend()
    }

    /// A new hint from the composition root. Takes effect at the next read; a hint equal to the
    /// endpoint already open is a no-op, and a hint superseded before the next read is coalesced.
    pub fn update_hint(&mut self, preferred_endpoint_id: Option<&str>) {
        let choice = EndpointChoice::from_hint(preferred_endpoint_id);
        if choice == self.selection.opened {
            if self.pending.take().is_some() {
                self.selection.coalesced_hints += 1;
            }
            return;
        }
        if let Some(previous) = self.pending.replace(choice) {
            if previous != *self.pending.as_ref().expect("just set") {
                self.selection.coalesced_hints += 1;
            }
        }
    }

    fn switch(&mut self, choice: EndpointChoice) -> Option<TrackOrigin> {
        match self.inner.reopen_device(choice.endpoint_id()) {
            Ok(origin) => {
                self.selection.opened = choice.clone();
                self.selection.history.push(choice);
                Some(origin)
            }
            Err(_) => {
                self.selection.failed_switches += 1;
                None
            }
        }
    }
}

impl<B: ActivationBackend> CaptureSource for MicEndpointSource<B> {
    fn origin(&self) -> TrackOrigin {
        self.inner.origin()
    }

    fn next(&mut self) -> SourceEvent {
        if let Some(choice) = self.pending.take() {
            if let Some(origin) = self.switch(choice) {
                return SourceEvent::FormatChanged(origin);
            }
        }
        self.inner.next()
    }

    fn take_discontinuities(&mut self) -> u32 {
        self.inner.take_discontinuities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasapi::fake::{FakeActivationBackend, FakeStream};
    use crate::wasapi::{StreamFormat, StreamRead};
    use ma_core_types::id::TypedId;
    use ma_core_types::timeline::{CaptureMode, ContaminationRisk, TrackSegment};
    use ma_core_types::TrackId;

    fn mono() -> StreamFormat {
        StreamFormat {
            sample_rate: 16_000,
            channels: 1,
        }
    }
    fn stream(v: i16) -> FakeStream {
        FakeStream::new(mono()).then(StreamRead::Samples(vec![v; 160]))
    }
    fn clock() -> OriginClock {
        let mut t = 0u64;
        Box::new(move || {
            t += 1;
            (1_756_857_600_000 + t as i64, t * 1_000_000_000)
        })
    }

    #[test]
    fn mic_endpoint_follows_supplied_session_endpoint() {
        let backend = FakeActivationBackend::default().device(Ok(stream(1)));
        let source =
            MicEndpointSource::open(backend, Some("{0.0.1.00000000}.{headset}"), clock()).unwrap();
        assert_eq!(
            source.selection().opened,
            EndpointChoice::Supplied("{0.0.1.00000000}.{headset}".into())
        );
        assert_eq!(source.origin().capture_mode, CaptureMode::Device);
        assert_eq!(source.origin().contamination_risk, ContaminationRisk::None);
        assert_eq!(
            source.backend().device_requests,
            vec![Some("{0.0.1.00000000}.{headset}".to_string())],
            "the backend was asked for exactly the supplied endpoint"
        );

        // No hint: the system default is opened and recorded as such.
        let backend = FakeActivationBackend::default().device(Ok(stream(2)));
        let source = MicEndpointSource::open(backend, None, clock()).unwrap();
        assert_eq!(source.selection().opened, EndpointChoice::SystemDefault);
        assert_eq!(source.backend().device_requests, vec![None]);
        assert_eq!(
            source.selection().history,
            vec![EndpointChoice::SystemDefault]
        );
    }

    #[test]
    fn backend_receives_the_supplied_endpoint_or_none() {
        let backend = FakeActivationBackend::default()
            .device(Ok(stream(1)))
            .device(Ok(stream(2)));
        let mut source = MicEndpointSource::open(backend, Some("{ep-a}"), clock()).unwrap();
        source.update_hint(None);
        let _ = source.next();
        assert_eq!(source.selection().opened, EndpointChoice::SystemDefault);
        assert_eq!(
            source.backend().device_requests,
            vec![Some("{ep-a}".to_string()), None]
        );
    }

    #[test]
    fn endpoint_change_opens_successor_track() {
        let backend = FakeActivationBackend::default()
            .device(Ok(stream(1)))
            .device(Ok(stream(2)));
        let mut source = MicEndpointSource::open(backend, Some("{ep-a}"), clock()).unwrap();
        let first_origin = source.origin();
        let segment = TrackSegment::new(TrackId::new(), first_origin.clone()).unwrap();
        assert_eq!(source.next(), SourceEvent::Samples(vec![1; 160]));

        // Two hints inside one selection window: only the latest is opened.
        source.update_hint(Some("{ep-transient}"));
        source.update_hint(Some("{ep-b}"));
        match source.next() {
            SourceEvent::FormatChanged(origin) => {
                assert!(origin.start_monotonic_ns > first_origin.start_monotonic_ns);
                assert_eq!(origin.capture_mode, CaptureMode::Device);
                let successor = segment
                    .open_successor(TrackId::new(), origin.clone())
                    .unwrap();
                assert_eq!(successor.origin, origin);
                assert_ne!(successor.track_id, segment.track_id);
            }
            other => panic!("expected a new origin, got {other:?}"),
        }
        assert_eq!(
            source.selection().opened,
            EndpointChoice::Supplied("{ep-b}".into())
        );
        assert_eq!(
            source.selection().history,
            vec![
                EndpointChoice::Supplied("{ep-a}".into()),
                EndpointChoice::Supplied("{ep-b}".into())
            ]
        );
        assert_eq!(source.selection().coalesced_hints, 1);
        assert_eq!(
            source.backend().device_requests,
            vec![Some("{ep-a}".to_string()), Some("{ep-b}".to_string())],
            "the transient hint was never opened"
        );
        assert_eq!(source.next(), SourceEvent::Samples(vec![2; 160]));

        // A hint equal to the open endpoint changes nothing.
        source.update_hint(Some("{ep-b}"));
        assert_eq!(source.next(), SourceEvent::Ended);
        assert_eq!(source.selection().history.len(), 2);
    }

    #[test]
    fn a_failed_switch_keeps_the_current_endpoint_open() {
        let backend = FakeActivationBackend::default()
            .device(Ok(stream(1)))
            .device(Err(ActivationError::NoEndpoint));
        let mut source = MicEndpointSource::open(backend, Some("{ep-a}"), clock()).unwrap();
        source.update_hint(Some("{ep-gone}"));
        assert_eq!(source.next(), SourceEvent::Samples(vec![1; 160]));
        assert_eq!(
            source.selection().opened,
            EndpointChoice::Supplied("{ep-a}".into())
        );
        assert_eq!(source.selection().failed_switches, 1);
    }

    #[test]
    fn lost_microphone_never_activates_render_loopback() {
        let lost = FakeStream::new(mono()).then(StreamRead::Lost);
        let render = FakeStream::new(mono()).then(StreamRead::Samples(vec![9; 160]));
        let backend = FakeActivationBackend::default()
            .device(Ok(lost))
            .system_loopback(Ok(render));
        let mut source = MicEndpointSource::open(backend, Some("{ep-a}"), clock()).unwrap();

        assert_eq!(source.next(), SourceEvent::Ended);
        assert_eq!(
            source.backend().system_activations,
            0,
            "a microphone loss must not switch the mic track to render loopback"
        );
        assert_eq!(source.origin().capture_mode, CaptureMode::Device);
    }
}
