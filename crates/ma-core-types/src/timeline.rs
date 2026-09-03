//! Sample-accurate session timeline (contract-session-timeline).
//!
//! Every position is a sample offset on its own track; user-facing timestamps are computed from
//! `start_sample / sample_rate` plus the track origin, never from concatenation order. Gaps are
//! first-class records so "audio we do not have" stays distinguishable from silence.

use crate::error::CoreError;
use crate::id::{ChunkId, ChunkSeq, TrackId};
use serde::{Deserialize, Serialize};

/// How the track's audio was obtained. Phase 1 discovers which mode a given application allows;
/// that outcome changes data, not schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    ProcessLoopback,
    SystemLoopback,
    Device,
}

/// Whether audio from other applications may be mixed into this track (PLAN section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContaminationRisk {
    None,
    PossibleOtherApps,
}

/// The origin of one track segment. A device format or endpoint change opens a new segment with
/// its own origin rather than reinterpreting old positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackOrigin {
    pub start_wall_utc_ms: i64,
    pub start_monotonic_ns: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub capture_mode: CaptureMode,
    pub contamination_risk: ContaminationRisk,
}

/// One durable chunk's position on its track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSpan {
    pub chunk_id: ChunkId,
    pub seq: ChunkSeq,
    pub start_sample: u64,
    pub len_samples: u64,
}

impl ChunkSpan {
    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.len_samples
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    ChunkLost,
    DeviceDiscontinuity,
    CaptureInterrupted,
    Other(String),
}

/// Audio the session does not have, as a first-class record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    pub from_sample: u64,
    pub to_sample: u64,
    pub reason: GapReason,
}

/// A session-relative instant with microsecond resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionTime {
    pub micros: u64,
}

impl SessionTime {
    pub fn as_secs_f64(self) -> f64 {
        self.micros as f64 / 1_000_000.0
    }
}

/// One track segment: an origin plus the chunks and gaps that tile its sample range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSegment {
    pub track_id: TrackId,
    pub origin: TrackOrigin,
    pub chunks: Vec<ChunkSpan>,
    pub gaps: Vec<Gap>,
}

impl TrackSegment {
    pub fn new(track_id: TrackId, origin: TrackOrigin) -> Result<Self, CoreError> {
        if origin.sample_rate == 0 {
            return Err(CoreError::ZeroSampleRate);
        }
        Ok(Self {
            track_id,
            origin,
            chunks: Vec::new(),
            gaps: Vec::new(),
        })
    }

    /// The first sample not yet covered by a chunk or a gap.
    pub fn end_sample(&self) -> u64 {
        self.chunks
            .iter()
            .map(ChunkSpan::end_sample)
            .chain(self.gaps.iter().map(|g| g.to_sample))
            .max()
            .unwrap_or(0)
    }

    /// The next dense chunk sequence number.
    pub fn next_seq(&self) -> ChunkSeq {
        self.chunks
            .iter()
            .map(|c| c.seq.next())
            .max()
            .unwrap_or(ChunkSeq(0))
    }

    /// Append a chunk immediately after the covered range.
    pub fn push_chunk(
        &mut self,
        chunk_id: ChunkId,
        len_samples: u64,
    ) -> Result<ChunkSpan, CoreError> {
        let start = self.end_sample();
        self.push_chunk_at(chunk_id, self.next_seq(), start, len_samples)
    }

    /// Place a chunk at an explicit position. Overlap with covered audio is a hard error:
    /// it means the writer lost track of position.
    pub fn push_chunk_at(
        &mut self,
        chunk_id: ChunkId,
        seq: ChunkSeq,
        start_sample: u64,
        len_samples: u64,
    ) -> Result<ChunkSpan, CoreError> {
        let previous_end = self.end_sample();
        if start_sample < previous_end {
            return Err(CoreError::ChunkOverlap {
                start_sample,
                previous_end,
            });
        }
        if len_samples == 0 {
            return Err(CoreError::TilingBroken {
                at_sample: start_sample,
            });
        }
        let span = ChunkSpan {
            chunk_id,
            seq,
            start_sample,
            len_samples,
        };
        self.chunks.push(span.clone());
        Ok(span)
    }

    /// Record missing audio from the current end up to `to_sample`.
    pub fn record_gap(&mut self, to_sample: u64, reason: GapReason) -> Result<Gap, CoreError> {
        let from_sample = self.end_sample();
        if to_sample <= from_sample {
            return Err(CoreError::InvalidGap {
                from_sample,
                to_sample,
            });
        }
        let gap = Gap {
            from_sample,
            to_sample,
            reason,
        };
        self.gaps.push(gap.clone());
        Ok(gap)
    }

    /// Verify that chunks and gaps tile `[0, end_sample)` with no hole and no overlap.
    pub fn validate_tiling(&self) -> Result<(), CoreError> {
        let mut spans: Vec<(u64, u64)> = self
            .chunks
            .iter()
            .map(|c| (c.start_sample, c.end_sample()))
            .chain(self.gaps.iter().map(|g| (g.from_sample, g.to_sample)))
            .collect();
        spans.sort_unstable();
        let mut cursor = 0u64;
        for (start, end) in spans {
            if start != cursor || end <= start {
                return Err(CoreError::TilingBroken {
                    at_sample: cursor.min(start),
                });
            }
            cursor = end;
        }
        Ok(())
    }

    /// Session-relative time of a sample on this track.
    pub fn time_of(&self, sample: u64) -> Result<SessionTime, CoreError> {
        if sample > self.end_sample() {
            return Err(CoreError::SampleOutOfRange { sample });
        }
        let micros = (sample as u128 * 1_000_000) / self.origin.sample_rate as u128;
        Ok(SessionTime {
            micros: micros as u64,
        })
    }

    /// Session-relative time of a chunk-local sample, found by chunk sequence number.
    pub fn chunk_local_time(
        &self,
        seq: ChunkSeq,
        local_sample: u64,
    ) -> Result<SessionTime, CoreError> {
        let chunk =
            self.chunks
                .iter()
                .find(|c| c.seq == seq)
                .ok_or(CoreError::SampleOutOfRange {
                    sample: local_sample,
                })?;
        if local_sample >= chunk.len_samples {
            return Err(CoreError::SampleOutOfRange {
                sample: chunk.start_sample + local_sample,
            });
        }
        self.time_of(chunk.start_sample + local_sample)
    }

    /// Wall-clock UTC milliseconds of a sample on this track.
    pub fn wall_utc_ms_at(&self, sample: u64) -> Result<i64, CoreError> {
        let t = self.time_of(sample)?;
        Ok(self.origin.start_wall_utc_ms + (t.micros / 1_000) as i64)
    }

    /// Close this segment and open its successor with a new origin (device format or endpoint
    /// change). Positions in this segment keep their original sample rate.
    pub fn open_successor(
        &self,
        track_id: TrackId,
        origin: TrackOrigin,
    ) -> Result<TrackSegment, CoreError> {
        TrackSegment::new(track_id, origin)
    }
}

/// The session view over independently originated tracks, aligned by wall-clock origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTimeline {
    pub session_start_wall_utc_ms: i64,
    pub alignment_uncertainty_ms: u32,
    pub tracks: Vec<TrackSegment>,
}

impl SessionTimeline {
    /// Session time of a sample on the given track. Sample `n` of one track is never assumed to be
    /// contemporaneous with sample `n` of another.
    pub fn session_time(
        &self,
        track: &TrackSegment,
        sample: u64,
    ) -> Result<SessionTime, CoreError> {
        let on_track = track.time_of(sample)?;
        let offset_ms = track.origin.start_wall_utc_ms - self.session_start_wall_utc_ms;
        let micros = on_track.micros as i128 + offset_ms as i128 * 1_000;
        if micros < 0 {
            return Err(CoreError::SampleOutOfRange { sample });
        }
        Ok(SessionTime {
            micros: micros as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::TypedId;
    use proptest::prelude::*;

    fn origin(rate: u32, wall_ms: i64) -> TrackOrigin {
        TrackOrigin {
            start_wall_utc_ms: wall_ms,
            start_monotonic_ns: 0,
            sample_rate: rate,
            channels: 1,
            capture_mode: CaptureMode::ProcessLoopback,
            contamination_risk: ContaminationRisk::None,
        }
    }

    const CHUNK: u64 = 30 * 16_000;

    #[test]
    fn timestamps_survive_missing_chunk() {
        let mut track = TrackSegment::new(TrackId::new(), origin(16_000, 0)).unwrap();
        track.push_chunk(ChunkId::new(), CHUNK).unwrap();
        track.push_chunk(ChunkId::new(), CHUNK).unwrap();
        // chunk 000002 was lost: a 30 s gap, recorded rather than concatenated away
        track.record_gap(3 * CHUNK, GapReason::ChunkLost).unwrap();
        let fourth = track
            .push_chunk_at(ChunkId::new(), ChunkSeq(3), 3 * CHUNK, CHUNK)
            .unwrap();
        assert_eq!(fourth.start_sample, 1_440_000);
        let t = track.chunk_local_time(ChunkSeq(3), 40_000).unwrap();
        assert_eq!(
            t.micros, 92_500_000,
            "2.5 s into the fourth chunk is session time 92.5 s"
        );
        let concatenated = (2 * CHUNK + 40_000) as f64 / 16_000.0;
        assert_ne!(
            t.as_secs_f64(),
            concatenated,
            "counting surviving chunks from zero would shift by 30 s"
        );
        assert!(track.validate_tiling().is_ok());
    }

    #[test]
    fn overlapping_chunk_is_a_hard_error() {
        let mut track = TrackSegment::new(TrackId::new(), origin(16_000, 0)).unwrap();
        track.push_chunk(ChunkId::new(), CHUNK).unwrap();
        let err = track
            .push_chunk_at(ChunkId::new(), ChunkSeq(1), CHUNK - 1, CHUNK)
            .unwrap_err();
        assert_eq!(
            err,
            CoreError::ChunkOverlap {
                start_sample: CHUNK - 1,
                previous_end: CHUNK
            }
        );
    }

    proptest! {
        #[test]
        fn chunks_and_gaps_tile_without_overlap(pieces in prop::collection::vec((any::<bool>(), 1u64..100_000), 1..40)) {
            let mut track = TrackSegment::new(TrackId::new(), origin(48_000, 0)).unwrap();
            for (is_gap, len) in pieces {
                if is_gap {
                    let end = track.end_sample() + len;
                    track.record_gap(end, GapReason::ChunkLost).unwrap();
                } else {
                    track.push_chunk(ChunkId::new(), len).unwrap();
                }
            }
            prop_assert!(track.validate_tiling().is_ok());
            let covered: u64 = track.chunks.iter().map(|c| c.len_samples).sum::<u64>() + track.gaps.iter().map(|g| g.to_sample - g.from_sample).sum::<u64>();
            prop_assert_eq!(covered, track.end_sample());
            // any sample inside the range has exactly one position
            let end = track.end_sample();
            prop_assert_eq!(track.time_of(end - 1).unwrap().micros, (end - 1) * 1_000_000 / 48_000);
        }
    }

    #[test]
    fn hole_between_records_breaks_tiling() {
        let mut track = TrackSegment::new(TrackId::new(), origin(16_000, 0)).unwrap();
        track.push_chunk(ChunkId::new(), CHUNK).unwrap();
        track
            .push_chunk_at(ChunkId::new(), ChunkSeq(1), 2 * CHUNK, CHUNK)
            .unwrap(); // silent hole, no gap record
        assert_eq!(
            track.validate_tiling(),
            Err(CoreError::TilingBroken { at_sample: CHUNK })
        );
    }

    #[test]
    fn tracks_have_independent_origins() {
        let mic = TrackSegment::new(TrackId::new(), origin(16_000, 1_000)).unwrap();
        let loopback = TrackSegment::new(TrackId::new(), origin(48_000, 1_750)).unwrap();
        let session = SessionTimeline {
            session_start_wall_utc_ms: 1_000,
            alignment_uncertainty_ms: 20,
            tracks: vec![mic.clone(), loopback.clone()],
        };
        let mut mic = mic;
        let mut loopback = loopback;
        mic.push_chunk(ChunkId::new(), 16_000 * 10).unwrap();
        loopback.push_chunk(ChunkId::new(), 48_000 * 10).unwrap();
        assert_eq!(
            session.session_time(&mic, 16_000).unwrap().micros,
            1_000_000
        );
        // sample 48_000 of the loopback track is one second after an origin that started 750 ms later
        assert_eq!(
            session.session_time(&loopback, 48_000).unwrap().micros,
            1_750_000
        );
        assert_ne!(
            session.session_time(&mic, 16_000).unwrap(),
            session.session_time(&loopback, 16_000).unwrap()
        );
    }

    #[test]
    fn format_change_opens_new_segment() {
        let mut first = TrackSegment::new(TrackId::new(), origin(16_000, 0)).unwrap();
        first.push_chunk(ChunkId::new(), CHUNK).unwrap();
        let second = first
            .open_successor(TrackId::new(), origin(48_000, 30_000))
            .unwrap();
        assert_eq!(second.end_sample(), 0);
        assert_eq!(
            first.time_of(CHUNK).unwrap().micros,
            30_000_000,
            "old positions keep the old rate"
        );
        assert_ne!(first.track_id, second.track_id);
    }
}
