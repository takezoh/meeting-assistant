//! WASAPI capture behind the [`CaptureSource`] seam (contract-process-loopback-capture).
//!
//! Activation has three typed outcomes, all legitimate and all observable: process loopback
//! (`CaptureMode::ProcessLoopback`, no contamination), the system-loopback fallback
//! (`CaptureMode::SystemLoopback`, `ContaminationRisk::PossibleOtherApps`), and the manual
//! device path (`CaptureMode::Device`) that stays constructible whatever the loopback outcome.
//! Every outcome yields a [`WasapiSource`] the durability path drives unchanged.
//!
//! The source pins its origin to `SAMPLE_RATE` and one channel: whatever the device mix format is,
//! samples are downmixed and resampled before they are emitted, so `CHUNK_SAMPLES` keeps meaning
//! thirty seconds. A backend whose format cannot be resampled is an activation error, never a track
//! whose origin rate differs from the writer's.
//!
//! The WASAPI calls live behind [`ActivationBackend`], whose live implementation is compiled only
//! on Windows; [`fake::FakeActivationBackend`] is the portable implementation of the same trait.

use crate::source::{CaptureSource, SourceEvent};
use crate::SAMPLE_RATE;
use ma_core_types::timeline::{CaptureMode, ContaminationRisk, TrackOrigin};

#[cfg(windows)]
mod manual_fallback;
#[cfg(windows)]
mod process_loopback;
#[cfg(windows)]
pub use process_loopback::WindowsActivationBackend;

pub mod leak_measure;
pub mod mic_endpoint;
pub use leak_measure::{
    measure_echo_return_loss, LeakMeasurementRecord, LeakOutcome, TrackSamples,
    RECORD_SCHEMA_VERSION, WINDOW_SECONDS,
};
pub use mic_endpoint::{EndpointChoice, EndpointSelection, MicEndpointSource};

/// The portable implementation of the activation seam: scripted outcomes and scripted streams.
pub mod fake {
    use super::{
        ActivationBackend, ActivationError, AudioStream, LoopbackTarget, StreamFormat, StreamRead,
    };
    use std::collections::VecDeque;

    /// A scripted stream: a fixed format and a queue of reads; `Ended` once the queue is empty.
    #[derive(Debug, Clone)]
    pub struct FakeStream {
        format: StreamFormat,
        reads: VecDeque<StreamRead>,
    }

    impl FakeStream {
        pub fn new(format: StreamFormat) -> Self {
            Self {
                format,
                reads: VecDeque::new(),
            }
        }
        pub fn then(mut self, read: StreamRead) -> Self {
            self.reads.push_back(read);
            self
        }
    }

    impl AudioStream for FakeStream {
        fn format(&self) -> StreamFormat {
            self.format
        }
        fn read(&mut self) -> StreamRead {
            match self.reads.pop_front() {
                Some(StreamRead::FormatChanged(f)) => {
                    self.format = f;
                    StreamRead::FormatChanged(f)
                }
                Some(read) => read,
                None => StreamRead::Ended,
            }
        }
    }

    type Scripted = VecDeque<Result<FakeStream, ActivationError>>;

    /// Scripted activation results per path. An exhausted script reports `Unavailable` for the
    /// loopback paths and `NoEndpoint` for the device path.
    #[derive(Debug, Default)]
    pub struct FakeActivationBackend {
        process: Scripted,
        system: Scripted,
        device: Scripted,
        pub process_activations: Vec<LoopbackTarget>,
        pub system_activations: u32,
        pub device_requests: Vec<Option<String>>,
    }

    impl FakeActivationBackend {
        pub fn process_loopback(mut self, r: Result<FakeStream, ActivationError>) -> Self {
            self.process.push_back(r);
            self
        }
        pub fn system_loopback(mut self, r: Result<FakeStream, ActivationError>) -> Self {
            self.system.push_back(r);
            self
        }
        pub fn device(mut self, r: Result<FakeStream, ActivationError>) -> Self {
            self.device.push_back(r);
            self
        }
    }

    fn boxed(
        r: Result<FakeStream, ActivationError>,
    ) -> Result<Box<dyn AudioStream>, ActivationError> {
        r.map(|s| Box::new(s) as Box<dyn AudioStream>)
    }

    impl ActivationBackend for FakeActivationBackend {
        fn activate_process_loopback(
            &mut self,
            target: LoopbackTarget,
        ) -> Result<Box<dyn AudioStream>, ActivationError> {
            self.process_activations.push(target);
            boxed(
                self.process
                    .pop_front()
                    .unwrap_or(Err(ActivationError::Unavailable)),
            )
        }
        fn activate_system_loopback(&mut self) -> Result<Box<dyn AudioStream>, ActivationError> {
            self.system_activations += 1;
            boxed(
                self.system
                    .pop_front()
                    .unwrap_or(Err(ActivationError::Unavailable)),
            )
        }
        fn open_device(
            &mut self,
            endpoint_id: Option<&str>,
        ) -> Result<Box<dyn AudioStream>, ActivationError> {
            self.device_requests.push(endpoint_id.map(str::to_string));
            boxed(
                self.device
                    .pop_front()
                    .unwrap_or(Err(ActivationError::NoEndpoint)),
            )
        }
    }
}

/// A stream's native interleaved format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// What one read from a backend stream produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamRead {
    /// Interleaved s16le samples in the stream's current format.
    Samples(Vec<i16>),
    /// The endpoint changed format; following samples use the new format.
    FormatChanged(StreamFormat),
    /// The activation was lost mid-session (device invalidated, target process gone).
    Lost,
    Ended,
}

/// A running capture stream as the backend hands it out.
pub trait AudioStream {
    fn format(&self) -> StreamFormat;
    fn read(&mut self) -> StreamRead;
    /// Buffer discontinuities the device reported since the last call (audio was lost between
    /// two reads). The default is for backends that cannot observe them.
    fn take_discontinuities(&mut self) -> u32 {
        0
    }
}

/// Which process to activate loopback for, and whether its whole process tree is included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackTarget {
    pub pid: u32,
    pub include_process_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationError {
    /// The activation type is not available on this host or for this process.
    Unavailable,
    /// The OS rejected the activation with this HRESULT.
    Failed { code: i32 },
    /// The endpoint's format cannot be brought to `SAMPLE_RATE` mono.
    UnsupportedFormat(StreamFormat),
    /// No endpoint exists for the requested path (no default render or capture device).
    NoEndpoint,
}

/// The three outcomes of opening a loopback source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    /// Process loopback activated for the target.
    Activated,
    /// Process loopback failed for `reason`; the source runs on system loopback.
    Fallback { reason: ActivationError },
    /// The manual device path.
    ManualOnly,
}

/// The seam the live WASAPI code and the portable fake both implement.
pub trait ActivationBackend {
    fn activate_process_loopback(
        &mut self,
        target: LoopbackTarget,
    ) -> Result<Box<dyn AudioStream>, ActivationError>;
    fn activate_system_loopback(&mut self) -> Result<Box<dyn AudioStream>, ActivationError>;
    /// Opens a capture endpoint; `None` selects the default capture device.
    fn open_device(
        &mut self,
        endpoint_id: Option<&str>,
    ) -> Result<Box<dyn AudioStream>, ActivationError>;
}

/// Wall-clock milliseconds and monotonic nanoseconds for a track origin.
pub type OriginClock = Box<dyn FnMut() -> (i64, u64)>;

/// The host clocks, with the monotonic zero at `origin` so every track and collector of one
/// session shares one time base.
pub fn origin_clock_from(origin: std::time::Instant) -> OriginClock {
    let start = origin;
    Box::new(move || {
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        (wall, start.elapsed().as_nanos() as u64)
    })
}

/// The host clocks with a fresh monotonic zero.
pub fn system_origin_clock() -> OriginClock {
    origin_clock_from(std::time::Instant::now())
}

/// Linear-interpolation resampler from an interleaved source format to `SAMPLE_RATE` mono.
#[derive(Debug, Clone)]
struct Resampler {
    src: StreamFormat,
    /// Fractional read position into the mono source, relative to `prev`.
    pos: f64,
    prev: Option<i16>,
}

impl Resampler {
    fn new(src: StreamFormat) -> Result<Self, ActivationError> {
        if src.channels == 0 || !(8_000..=384_000).contains(&src.sample_rate) {
            return Err(ActivationError::UnsupportedFormat(src));
        }
        Ok(Self {
            src,
            pos: 0.0,
            prev: None,
        })
    }

    fn downmix(&self, interleaved: &[i16]) -> Vec<i16> {
        let ch = self.src.channels as usize;
        interleaved
            .chunks_exact(ch)
            .map(|frame| (frame.iter().map(|s| *s as i32).sum::<i32>() / ch as i32) as i16)
            .collect()
    }

    fn push(&mut self, interleaved: &[i16]) -> Vec<i16> {
        let mono = self.downmix(interleaved);
        if mono.is_empty() {
            return Vec::new();
        }
        if self.src.sample_rate == SAMPLE_RATE && self.src.channels == 1 {
            return mono;
        }
        let mut buf: Vec<i16> = Vec::with_capacity(mono.len() + 1);
        if let Some(p) = self.prev {
            buf.push(p);
        } else {
            // First block: start at the first sample.
            self.pos = 0.0;
        }
        buf.extend_from_slice(&mono);
        let ratio = self.src.sample_rate as f64 / SAMPLE_RATE as f64;
        let mut out = Vec::with_capacity((mono.len() as f64 / ratio) as usize + 1);
        while (self.pos.floor() as usize) + 1 < buf.len() {
            let i = self.pos.floor() as usize;
            let frac = self.pos - i as f64;
            let a = buf[i] as f64;
            let b = buf[i + 1] as f64;
            out.push((a + (b - a) * frac).round().clamp(-32_768.0, 32_767.0) as i16);
            self.pos += ratio;
        }
        // Keep the last consumed source sample as the interpolation anchor for the next block.
        // A block shorter than one output step leaves the read position past the buffer; the
        // anchor is then the last sample and the fractional position is measured from it.
        let consumed = (self.pos.floor() as usize).min(buf.len() - 1);
        self.prev = Some(buf[consumed]);
        self.pos -= consumed as f64;
        out
    }
}

/// A [`CaptureSource`] over a backend stream, pinned to `SAMPLE_RATE` mono.
pub struct WasapiSource<B: ActivationBackend> {
    backend: B,
    stream: Box<dyn AudioStream>,
    resampler: Resampler,
    origin: TrackOrigin,
    outcome: ActivationOutcome,
    clock: OriginClock,
    ended: bool,
}

fn origin_for(mode: CaptureMode, risk: ContaminationRisk, clock: &mut OriginClock) -> TrackOrigin {
    let (wall, mono) = clock();
    TrackOrigin {
        start_wall_utc_ms: wall,
        start_monotonic_ns: mono,
        sample_rate: SAMPLE_RATE,
        channels: 1,
        capture_mode: mode,
        contamination_risk: risk,
    }
}

impl<B: ActivationBackend> WasapiSource<B> {
    /// Process loopback for `target`, falling back to system loopback with the contamination
    /// risk recorded. Fails only when neither activation succeeds.
    pub fn open_process_loopback(
        mut backend: B,
        target: LoopbackTarget,
        mut clock: OriginClock,
    ) -> Result<Self, ActivationError> {
        let (stream, mode, risk, outcome) = match backend.activate_process_loopback(target) {
            Ok(stream) => (
                stream,
                CaptureMode::ProcessLoopback,
                ContaminationRisk::None,
                ActivationOutcome::Activated,
            ),
            Err(reason) => {
                let stream = backend.activate_system_loopback()?;
                (
                    stream,
                    CaptureMode::SystemLoopback,
                    ContaminationRisk::PossibleOtherApps,
                    ActivationOutcome::Fallback { reason },
                )
            }
        };
        let resampler = Resampler::new(stream.format())?;
        let origin = origin_for(mode, risk, &mut clock);
        Ok(Self {
            backend,
            stream,
            resampler,
            origin,
            outcome,
            clock,
            ended: false,
        })
    }

    /// The manual path: a capture endpoint chosen by the user (or the default one), in Device mode.
    pub fn open_manual_device(
        mut backend: B,
        endpoint_id: Option<&str>,
        mut clock: OriginClock,
    ) -> Result<Self, ActivationError> {
        let stream = backend.open_device(endpoint_id)?;
        let resampler = Resampler::new(stream.format())?;
        let origin = origin_for(CaptureMode::Device, ContaminationRisk::None, &mut clock);
        Ok(Self {
            backend,
            stream,
            resampler,
            origin,
            outcome: ActivationOutcome::ManualOnly,
            clock,
            ended: false,
        })
    }

    pub fn outcome(&self) -> &ActivationOutcome {
        &self.outcome
    }

    /// Reopens the Device-mode source on another capture endpoint (`None` = system default) and
    /// returns the new origin. On failure the current stream stays open and unchanged.
    pub fn reopen_device(
        &mut self,
        endpoint_id: Option<&str>,
    ) -> Result<TrackOrigin, ActivationError> {
        let stream = self.backend.open_device(endpoint_id)?;
        let resampler = Resampler::new(stream.format())?;
        self.stream = stream;
        self.resampler = resampler;
        self.origin = origin_for(
            CaptureMode::Device,
            ContaminationRisk::None,
            &mut self.clock,
        );
        self.outcome = ActivationOutcome::ManualOnly;
        self.ended = false;
        Ok(self.origin.clone())
    }

    /// The backend stream's native format (before the pin).
    pub fn native_format(&self) -> StreamFormat {
        self.stream.format()
    }

    /// The activation backend, for callers and tests that need to observe what was requested.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Device-reported buffer discontinuities since the last call: audio the device dropped
    /// between two reads, which the caller records as a capture gap.
    pub fn take_discontinuities(&mut self) -> u32 {
        self.stream.take_discontinuities()
    }

    /// A mid-session loss of the activation: reopen on system loopback with a new origin so the
    /// durability path opens a successor segment instead of writing silence.
    fn reopen_after_loss(&mut self) -> SourceEvent {
        // Device-mode sources carry microphone input. Falling back to render loopback here would
        // silently change the meaning of the track from microphone audio to speaker output.
        // MicEndpointSource can explicitly reopen a capture device when it receives a new hint;
        // until then, loss is terminal for this stream.
        if self.origin.capture_mode == CaptureMode::Device {
            self.ended = true;
            return SourceEvent::Ended;
        }
        match self.backend.activate_system_loopback() {
            Ok(stream) => match Resampler::new(stream.format()) {
                Ok(resampler) => {
                    self.stream = stream;
                    self.resampler = resampler;
                    self.origin = origin_for(
                        CaptureMode::SystemLoopback,
                        ContaminationRisk::PossibleOtherApps,
                        &mut self.clock,
                    );
                    self.outcome = ActivationOutcome::Fallback {
                        reason: ActivationError::Unavailable,
                    };
                    SourceEvent::FormatChanged(self.origin.clone())
                }
                Err(_) => {
                    self.ended = true;
                    SourceEvent::Ended
                }
            },
            Err(_) => {
                self.ended = true;
                SourceEvent::Ended
            }
        }
    }
}

impl<B: ActivationBackend> CaptureSource for WasapiSource<B> {
    fn origin(&self) -> TrackOrigin {
        self.origin.clone()
    }

    fn next(&mut self) -> SourceEvent {
        if self.ended {
            return SourceEvent::Ended;
        }
        loop {
            match self.stream.read() {
                StreamRead::Samples(interleaved) => {
                    let out = self.resampler.push(&interleaved);
                    if out.is_empty() {
                        continue;
                    }
                    return SourceEvent::Samples(out);
                }
                StreamRead::FormatChanged(format) => match Resampler::new(format) {
                    // The pin holds: the origin does not change, only the conversion does.
                    Ok(resampler) => self.resampler = resampler,
                    Err(_) => return self.reopen_after_loss(),
                },
                StreamRead::Lost => return self.reopen_after_loss(),
                StreamRead::Ended => {
                    self.ended = true;
                    return SourceEvent::Ended;
                }
            }
        }
    }

    fn take_discontinuities(&mut self) -> u32 {
        WasapiSource::take_discontinuities(self)
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeActivationBackend, FakeStream};
    use super::*;

    fn clock() -> OriginClock {
        Box::new(|| (1_756_857_600_000, 1_000_000_000))
    }

    fn stereo_48k(blocks: usize, frames: usize) -> FakeStream {
        let mut s = FakeStream::new(StreamFormat {
            sample_rate: 48_000,
            channels: 2,
        });
        for b in 0..blocks {
            let block: Vec<i16> = (0..frames)
                .flat_map(|i| {
                    let v = ((b * frames + i) % 1000) as i16;
                    [v, v]
                })
                .collect();
            s = s.then(StreamRead::Samples(block));
        }
        s
    }

    #[test]
    fn process_loopback_falls_back_to_system_loopback_on_activation_failure() {
        let backend = FakeActivationBackend::default()
            .process_loopback(Err(ActivationError::Unavailable))
            .system_loopback(Ok(stereo_48k(1, 480)));
        let target = LoopbackTarget {
            pid: 4242,
            include_process_tree: true,
        };
        let source = WasapiSource::open_process_loopback(backend, target, clock()).unwrap();
        assert_eq!(
            *source.outcome(),
            ActivationOutcome::Fallback {
                reason: ActivationError::Unavailable
            }
        );
        let origin = source.origin();
        assert_eq!(origin.capture_mode, CaptureMode::SystemLoopback);
        assert_eq!(
            origin.contamination_risk,
            ContaminationRisk::PossibleOtherApps
        );

        let backend = FakeActivationBackend::default().process_loopback(Ok(stereo_48k(1, 480)));
        let source = WasapiSource::open_process_loopback(backend, target, clock()).unwrap();
        assert_eq!(*source.outcome(), ActivationOutcome::Activated);
        assert_eq!(source.origin().capture_mode, CaptureMode::ProcessLoopback);
        assert_eq!(source.origin().contamination_risk, ContaminationRisk::None);

        // A failed activation is reported with its code, not swallowed.
        let backend = FakeActivationBackend::default()
            .process_loopback(Err(ActivationError::Failed { code: -2004287480 }))
            .system_loopback(Ok(stereo_48k(1, 480)));
        let source = WasapiSource::open_process_loopback(backend, target, clock()).unwrap();
        assert!(matches!(
            source.outcome(),
            ActivationOutcome::Fallback {
                reason: ActivationError::Failed { code: -2004287480 }
            }
        ));
    }

    #[test]
    fn manual_capture_source_available_independent_of_loopback_outcome() {
        // Both loopback paths fail: the loopback source cannot open ...
        let backend = FakeActivationBackend::default()
            .process_loopback(Err(ActivationError::Unavailable))
            .system_loopback(Err(ActivationError::NoEndpoint));
        let target = LoopbackTarget {
            pid: 1,
            include_process_tree: false,
        };
        let err = WasapiSource::open_process_loopback(backend, target, clock()).err();
        assert_eq!(err, Some(ActivationError::NoEndpoint));
        // ... and the manual Device-mode source opens anyway, in the same process.
        let backend = FakeActivationBackend::default()
            .process_loopback(Err(ActivationError::Unavailable))
            .system_loopback(Err(ActivationError::NoEndpoint))
            .device(Ok(FakeStream::new(StreamFormat {
                sample_rate: 16_000,
                channels: 1,
            })
            .then(StreamRead::Samples(vec![1, 2, 3]))));
        let mut source =
            WasapiSource::open_manual_device(backend, Some("{endpoint-1}"), clock()).unwrap();
        assert_eq!(*source.outcome(), ActivationOutcome::ManualOnly);
        assert_eq!(source.origin().capture_mode, CaptureMode::Device);
        assert_eq!(source.origin().contamination_risk, ContaminationRisk::None);
        assert_eq!(source.next(), SourceEvent::Samples(vec![1, 2, 3]));
        assert_eq!(source.next(), SourceEvent::Ended);
    }

    #[test]
    fn wasapi_origin_is_pinned_to_sample_rate_and_mono() {
        let backend = FakeActivationBackend::default().process_loopback(Ok(stereo_48k(3, 480)));
        let target = LoopbackTarget {
            pid: 7,
            include_process_tree: true,
        };
        let mut source = WasapiSource::open_process_loopback(backend, target, clock()).unwrap();
        assert_eq!(
            source.native_format(),
            StreamFormat {
                sample_rate: 48_000,
                channels: 2
            }
        );
        let origin = source.origin();
        assert_eq!(origin.sample_rate, SAMPLE_RATE);
        assert_eq!(origin.channels, 1);
        let mut total = 0usize;
        loop {
            match source.next() {
                SourceEvent::Samples(s) => total += s.len(),
                SourceEvent::Ended => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        // 3 blocks x 480 stereo frames at 48 kHz -> 1440 mono frames -> 480 at 16 kHz (+-1).
        assert!((479..=481).contains(&total), "got {total} samples");

        // A format that cannot be resampled is an activation error, not a mismatched origin.
        let backend =
            FakeActivationBackend::default().process_loopback(Ok(FakeStream::new(StreamFormat {
                sample_rate: 48_000,
                channels: 0,
            })));
        let err = WasapiSource::open_process_loopback(backend, target, clock()).err();
        assert_eq!(
            err,
            Some(ActivationError::UnsupportedFormat(StreamFormat {
                sample_rate: 48_000,
                channels: 0
            }))
        );
    }

    #[test]
    fn mid_session_loss_reopens_on_system_loopback_with_a_new_origin() {
        let process = FakeStream::new(StreamFormat {
            sample_rate: 16_000,
            channels: 1,
        })
        .then(StreamRead::Samples(vec![5; 160]))
        .then(StreamRead::Lost);
        let system = FakeStream::new(StreamFormat {
            sample_rate: 16_000,
            channels: 1,
        })
        .then(StreamRead::Samples(vec![9; 160]));
        let backend = FakeActivationBackend::default()
            .process_loopback(Ok(process))
            .system_loopback(Ok(system));
        let mut ticks = 0u64;
        let clock: OriginClock = Box::new(move || {
            ticks += 1;
            (1_756_857_600_000 + ticks as i64, ticks * 1_000_000_000)
        });
        let target = LoopbackTarget {
            pid: 7,
            include_process_tree: true,
        };
        let mut source = WasapiSource::open_process_loopback(backend, target, clock).unwrap();
        let first = source.origin();
        assert_eq!(source.next(), SourceEvent::Samples(vec![5; 160]));
        match source.next() {
            SourceEvent::FormatChanged(origin) => {
                assert_eq!(origin.capture_mode, CaptureMode::SystemLoopback);
                assert_eq!(
                    origin.contamination_risk,
                    ContaminationRisk::PossibleOtherApps
                );
                assert!(origin.start_monotonic_ns > first.start_monotonic_ns);
                assert_eq!(origin, source.origin());
            }
            other => panic!("expected a new origin, got {other:?}"),
        }
        assert_eq!(source.next(), SourceEvent::Samples(vec![9; 160]));
        assert_eq!(source.next(), SourceEvent::Ended);
    }

    #[test]
    fn resampler_preserves_a_constant_and_a_ramp() {
        let mut r = Resampler::new(StreamFormat {
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
        let constant: Vec<i16> = vec![1000; 96];
        assert!(r.push(&constant).iter().all(|s| *s == 1000));
        let mut r = Resampler::new(StreamFormat {
            sample_rate: 32_000,
            channels: 1,
        })
        .unwrap();
        let ramp: Vec<i16> = (0..64).collect();
        let out = r.push(&ramp);
        assert_eq!(out.len(), 32);
        assert!(out.windows(2).all(|w| w[1] - w[0] == 2));
    }
}
