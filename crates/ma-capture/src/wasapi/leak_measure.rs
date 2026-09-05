//! Echo return loss between the paired loopback and microphone tracks
//! (contract-echo-leak-measurement, adr-20260904-echo-leak-measurement-representation).
//!
//! One statistic, one window, one alignment basis:
//!
//! - `erl_db = rms_dbfs(loopback over W) - rms_dbfs(microphone over W)`; higher means less leak;
//! - `W` is the first contiguous sixty-second window in which the loopback track's RMS is at least
//!   −40 dBFS and no twenty-millisecond microphone frame exceeds −20 dBFS;
//! - `W` is located on each track by that track's own origin, so the measurement never depends on
//!   a sample alignment the recording model refuses to promise.
//!
//! The result is capture-side data in a per-application record. No `Signal` is emitted and no
//! `Payload` field is written.

use crate::SAMPLE_RATE;
use serde::{Deserialize, Serialize};

pub const WINDOW_SECONDS: u64 = 60;
pub const FRAME_MS: u64 = 20;
/// The loopback track must carry at least this much energy over `W` (the application is producing audio).
pub const LOOPBACK_ACTIVE_MIN_DBFS: f64 = -40.0;
/// No microphone frame in `W` may exceed this level (no local speech).
pub const LOCAL_SPEECH_MAX_DBFS: f64 = -20.0;
/// Beyond this alignment uncertainty the session is inconclusive for a windowed comparison.
pub const MAX_ALIGNMENT_UNCERTAINTY_MS: u32 = 1_000;
/// Window admission is retried at this step along the common time span.
const SEARCH_STEP_SECONDS: u64 = 1;
/// Digital silence has no level; it is reported as this floor.
const SILENCE_FLOOR_DBFS: f64 = -120.0;

/// One 16 kHz mono track: its origin on the monotonic clock and its samples.
#[derive(Debug, Clone, Copy)]
pub struct TrackSamples<'a> {
    pub start_monotonic_ns: u64,
    pub samples: &'a [i16],
}

/// The measurement outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LeakOutcome {
    Measured {
        erl_db: f64,
        loopback_start_sample: u64,
        microphone_start_sample: u64,
        loopback_rms_dbfs: f64,
        microphone_rms_dbfs: f64,
        alignment_uncertainty_ms: u32,
    },
    NoQualifyingWindow,
    InconclusiveAlignment {
        alignment_uncertainty_ms: u32,
    },
}

/// The per-application record the Windows-tier procedure commits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeakMeasurementRecord {
    pub schema_version: u32,
    /// The adapter table id of the application measured.
    pub application_id: String,
    pub window_seconds: u64,
    #[serde(flatten)]
    pub outcome: LeakOutcome,
}

pub const RECORD_SCHEMA_VERSION: u32 = 1;

/// RMS level of `samples` in dBFS (full scale = 32768); digital silence is the floor.
pub fn rms_dbfs(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return SILENCE_FLOOR_DBFS;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sum / samples.len() as f64).sqrt();
    if rms <= 0.0 {
        SILENCE_FLOOR_DBFS
    } else {
        (20.0 * (rms / 32_768.0).log10()).max(SILENCE_FLOOR_DBFS)
    }
}

fn sample_at(track: &TrackSamples<'_>, t_ns: u64) -> Option<u64> {
    if t_ns < track.start_monotonic_ns {
        return None;
    }
    let offset_ns = t_ns - track.start_monotonic_ns;
    Some(
        offset_ns / 1_000_000_000 * SAMPLE_RATE as u64
            + (offset_ns % 1_000_000_000) * SAMPLE_RATE as u64 / 1_000_000_000,
    )
}

fn window<'a>(track: &TrackSamples<'a>, start_sample: u64, len: u64) -> Option<&'a [i16]> {
    let start = usize::try_from(start_sample).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    track.samples.get(start..end)
}

/// Measures the echo return loss over the first qualifying sixty-second window.
pub fn measure_echo_return_loss(
    application_id: &str,
    loopback: &TrackSamples<'_>,
    microphone: &TrackSamples<'_>,
    alignment_uncertainty_ms: u32,
) -> LeakMeasurementRecord {
    let record = |outcome| LeakMeasurementRecord {
        schema_version: RECORD_SCHEMA_VERSION,
        application_id: application_id.to_string(),
        window_seconds: WINDOW_SECONDS,
        outcome,
    };
    if alignment_uncertainty_ms > MAX_ALIGNMENT_UNCERTAINTY_MS {
        return record(LeakOutcome::InconclusiveAlignment {
            alignment_uncertainty_ms,
        });
    }
    let window_len = WINDOW_SECONDS * SAMPLE_RATE as u64;
    let frame_len = (FRAME_MS * SAMPLE_RATE as u64 / 1_000) as usize;
    let mut t_ns = loopback
        .start_monotonic_ns
        .max(microphone.start_monotonic_ns);
    while let (Some(l_start), Some(m_start)) =
        (sample_at(loopback, t_ns), sample_at(microphone, t_ns))
    {
        let (Some(l_win), Some(m_win)) = (
            window(loopback, l_start, window_len),
            window(microphone, m_start, window_len),
        ) else {
            break;
        };
        let loopback_rms = rms_dbfs(l_win);
        let loopback_active = loopback_rms >= LOOPBACK_ACTIVE_MIN_DBFS;
        let no_local_speech = m_win
            .chunks(frame_len)
            .all(|frame| rms_dbfs(frame) <= LOCAL_SPEECH_MAX_DBFS);
        if loopback_active && no_local_speech {
            let microphone_rms = rms_dbfs(m_win);
            return record(LeakOutcome::Measured {
                erl_db: loopback_rms - microphone_rms,
                loopback_start_sample: l_start,
                microphone_start_sample: m_start,
                loopback_rms_dbfs: loopback_rms,
                microphone_rms_dbfs: microphone_rms,
                alignment_uncertainty_ms,
            });
        }
        t_ns += SEARCH_STEP_SECONDS * 1_000_000_000;
    }
    record(LeakOutcome::NoQualifyingWindow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: u64, amplitude: f64, offset: usize) -> Vec<i16> {
        let n = (seconds * SAMPLE_RATE as u64) as usize;
        (0..n)
            .map(|i| {
                let phase =
                    ((i + offset) as f64) * 440.0 * std::f64::consts::TAU / SAMPLE_RATE as f64;
                (amplitude * phase.sin()).round() as i16
            })
            .collect()
    }

    #[test]
    fn leak_erl_from_paired_fixture_tracks() {
        // Loopback at -12 dBFS peak; the microphone hears it 18 dB lower, offset by 37 ms of
        // alignment difference the origins record.
        let loop_amp = 32_768.0 * 10f64.powf(-12.0 / 20.0);
        let mic_amp = loop_amp * 10f64.powf(-18.0 / 20.0);
        let loopback_samples = tone(65, loop_amp, 0);
        let mic_samples = tone(65, mic_amp, 592);
        let loopback = TrackSamples {
            start_monotonic_ns: 5_000_000_000,
            samples: &loopback_samples,
        };
        let microphone = TrackSamples {
            start_monotonic_ns: 5_037_000_000,
            samples: &mic_samples,
        };
        let record = measure_echo_return_loss("example-meetings", &loopback, &microphone, 40);
        match &record.outcome {
            LeakOutcome::Measured {
                erl_db,
                loopback_start_sample,
                microphone_start_sample,
                loopback_rms_dbfs,
                microphone_rms_dbfs,
                alignment_uncertainty_ms,
            } => {
                assert!((erl_db - 18.0).abs() <= 1.0, "erl {erl_db}");
                // W is located on each track by its own origin: the microphone started 37 ms later.
                assert_eq!(*microphone_start_sample, 0);
                assert_eq!(*loopback_start_sample, 592);
                assert!(
                    (*loopback_rms_dbfs - (-15.0)).abs() < 0.5,
                    "{loopback_rms_dbfs}"
                );
                assert!(
                    (*microphone_rms_dbfs - (-33.0)).abs() < 0.5,
                    "{microphone_rms_dbfs}"
                );
                assert_eq!(*alignment_uncertainty_ms, 40);
            }
            other => panic!("expected a measurement, got {other:?}"),
        }
        assert_eq!(record.window_seconds, 60);
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["outcome"], "measured");
        assert_eq!(json["application_id"], "example-meetings");
        let back: LeakMeasurementRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn leak_measurement_reports_no_qualifying_window() {
        let loop_amp = 32_768.0 * 10f64.powf(-12.0 / 20.0);
        let loopback_samples = tone(120, loop_amp, 0);
        // Local speech: a -6 dBFS burst of 20 ms every 30 s, so every 60 s window contains one.
        let mut mic_samples = tone(120, loop_amp * 0.1, 0);
        let frame = (FRAME_MS * SAMPLE_RATE as u64 / 1_000) as usize;
        for burst_start in (0..mic_samples.len()).step_by(30 * SAMPLE_RATE as usize) {
            for s in mic_samples.iter_mut().skip(burst_start).take(frame) {
                *s = 16_000;
            }
        }
        let loopback = TrackSamples {
            start_monotonic_ns: 0,
            samples: &loopback_samples,
        };
        let microphone = TrackSamples {
            start_monotonic_ns: 0,
            samples: &mic_samples,
        };
        let record = measure_echo_return_loss("example-browser", &loopback, &microphone, 10);
        assert_eq!(record.outcome, LeakOutcome::NoQualifyingWindow);

        // A silent loopback track never qualifies either: the application produced no audio.
        let silent = vec![0i16; 120 * SAMPLE_RATE as usize];
        let quiet_mic = vec![0i16; 120 * SAMPLE_RATE as usize];
        let record = measure_echo_return_loss(
            "example-browser",
            &TrackSamples {
                start_monotonic_ns: 0,
                samples: &silent,
            },
            &TrackSamples {
                start_monotonic_ns: 0,
                samples: &quiet_mic,
            },
            10,
        );
        assert_eq!(record.outcome, LeakOutcome::NoQualifyingWindow);

        // Excessive alignment uncertainty is inconclusive before any window is examined.
        let record = measure_echo_return_loss("example-browser", &loopback, &microphone, 1_500);
        assert_eq!(
            record.outcome,
            LeakOutcome::InconclusiveAlignment {
                alignment_uncertainty_ms: 1_500
            }
        );
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["outcome"], "inconclusive_alignment");
    }

    #[test]
    fn rms_of_silence_is_the_floor() {
        assert_eq!(rms_dbfs(&[]), -120.0);
        assert_eq!(rms_dbfs(&[0, 0, 0]), -120.0);
        assert!((rms_dbfs(&[32_767, -32_767]) - 0.0).abs() < 0.01);
    }
}
