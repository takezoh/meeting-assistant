//! The capture source seam. WASAPI arrives in Phase 1 behind this trait; `SyntheticSource` is
//! deterministic so chunk boundaries, gaps, recovery and consolidation are testable anywhere.

use ma_core_types::timeline::{CaptureMode, ContaminationRisk, TrackOrigin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceEvent {
    /// Interleaved s16le samples in the current origin's format.
    Samples(Vec<i16>),
    /// The device changed format or endpoint: a new track segment with this origin follows.
    FormatChanged(TrackOrigin),
    Ended,
}

pub trait CaptureSource {
    fn origin(&self) -> TrackOrigin;
    fn next(&mut self) -> SourceEvent;
    /// Device-reported discontinuities since the previous call. Sources that cannot observe
    /// discontinuities keep the default zero count.
    fn take_discontinuities(&mut self) -> u32 {
        0
    }
}

/// A deterministic PCM ramp: sample `i` is `((i * 37) mod 65536) - 32768`.
#[derive(Debug, Clone)]
pub struct SyntheticSource {
    origin: TrackOrigin,
    total_samples: u64,
    block: usize,
    produced: u64,
    format_change_at: Option<(u64, TrackOrigin)>,
    changed: bool,
}

impl SyntheticSource {
    pub fn new(sample_rate: u32, total_samples: u64, block: usize) -> SyntheticSource {
        SyntheticSource {
            origin: TrackOrigin {
                start_wall_utc_ms: 1_756_857_600_000,
                start_monotonic_ns: 1_000_000_000,
                sample_rate,
                channels: 1,
                capture_mode: CaptureMode::Device,
                contamination_risk: ContaminationRisk::None,
            },
            total_samples,
            block,
            produced: 0,
            format_change_at: None,
            changed: false,
        }
    }

    /// Switch to `origin` once `at_sample` samples have been produced (a Bluetooth reconnect).
    pub fn with_format_change(mut self, at_sample: u64, origin: TrackOrigin) -> SyntheticSource {
        self.format_change_at = Some((at_sample, origin));
        self
    }

    pub fn sample(i: u64) -> i16 {
        (((i.wrapping_mul(37)) % 65_536) as i64 - 32_768) as i16
    }

    pub fn produced(&self) -> u64 {
        self.produced
    }
}

impl CaptureSource for SyntheticSource {
    fn origin(&self) -> TrackOrigin {
        self.origin.clone()
    }
    fn next(&mut self) -> SourceEvent {
        if let Some((at, origin)) = &self.format_change_at {
            if !self.changed && self.produced >= *at {
                self.changed = true;
                self.origin = origin.clone();
                return SourceEvent::FormatChanged(origin.clone());
            }
        }
        if self.produced >= self.total_samples {
            return SourceEvent::Ended;
        }
        let mut n = self
            .block
            .min((self.total_samples - self.produced) as usize);
        if let Some((at, _)) = &self.format_change_at {
            if !self.changed && self.produced < *at {
                n = n.min((*at - self.produced) as usize);
            }
        }
        let samples = (0..n as u64)
            .map(|k| Self::sample(self.produced + k))
            .collect();
        self.produced += n as u64;
        SourceEvent::Samples(samples)
    }
}
