//! Samples → durable 30 s chunks in the order write `.part` → flush → rename → manifest → fsync.
//! The writer never blocks on anything but the filesystem seam it is given, and when that stalls it
//! drops audio loudly: a bounded 60 s queue, explicit gap records, a `capture.degraded` event with
//! the reason (`disk_backpressure`, or `disk_full` once the filesystem said so).

use crate::manifest::{ChunkManifest, ChunkRecord, ManifestEvent, ManifestEventKind};
use crate::wav;
use ma_core_types::timeline::{GapReason, TrackOrigin};
use ma_core_types::TrackId;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub const SAMPLE_RATE: u32 = 16_000;
/// 30 s at 16 kHz mono.
pub const CHUNK_SAMPLES: usize = 480_000;
/// 60 s per track: two whole chunks.
pub const QUEUE_CAP_SAMPLES: usize = 2 * CHUNK_SAMPLES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// The filesystem is not accepting writes right now; nothing was written.
    Stalled,
    DiskFull,
    Io,
}

/// The filesystem seam. `RealFs` is the real thing; tests inject stalls and full disks.
pub trait ChunkFs {
    fn write_part(&mut self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;
    fn rename_in(&mut self, part: &Path, final_path: &Path) -> Result<(), FsError>;
    fn save_manifest(&mut self, dir: &Path, manifest: &ChunkManifest) -> Result<(), FsError>;
}

#[derive(Debug, Default)]
pub struct RealFs;

/// ENOSPC (Linux 28), ERROR_HANDLE_DISK_FULL (Windows 39), ERROR_DISK_FULL (Windows 112).
pub fn is_disk_full(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(28) | Some(39) | Some(112))
        || err.kind() == std::io::ErrorKind::StorageFull
}

impl ChunkFs for RealFs {
    fn write_part(&mut self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        use std::io::Write;
        let map = |e: std::io::Error| {
            if is_disk_full(&e) {
                FsError::DiskFull
            } else {
                FsError::Io
            }
        };
        let mut f = std::fs::File::create(path).map_err(map)?;
        f.write_all(bytes).map_err(map)?;
        f.sync_all().map_err(map)
    }
    fn rename_in(&mut self, part: &Path, final_path: &Path) -> Result<(), FsError> {
        std::fs::rename(part, final_path).map_err(|_| FsError::Io)?;
        if let Some(dir) = final_path.parent() {
            if let Ok(d) = std::fs::File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    }
    fn save_manifest(&mut self, dir: &Path, manifest: &ChunkManifest) -> Result<(), FsError> {
        manifest.save(dir).map_err(|e| {
            if is_disk_full(&e) {
                FsError::DiskFull
            } else {
                FsError::Io
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradedReason {
    DiskBackpressure,
    DiskFull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriterEvent {
    ChunkDurable {
        seq: u32,
        samples: u64,
    },
    Degraded {
        reason: DegradedReason,
        dropped_samples: u64,
    },
    /// A device format or endpoint change ended this track and opened its successor.
    SuccessorOpened {
        successor: TrackId,
    },
}

/// Audio dropped while the queue was full. It sits after `queue_offset` queued samples; samples
/// accepted after that stall ended follow it in the queue. Several may be pending at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingGap {
    queue_offset: usize,
    dropped: u64,
    gap_index: usize,
}

pub struct ChunkWriter {
    track_dir: PathBuf,
    manifest: ChunkManifest,
    queue: VecDeque<i16>,
    /// Track position of the first queued sample.
    position: u64,
    next_seq: u32,
    pub events: Vec<WriterEvent>,
    pending: VecDeque<PendingGap>,
    /// The filesystem reported a full disk on the last write; drops are attributed to it.
    disk_full: bool,
}

impl ChunkWriter {
    /// Open (or resume) the writer for one track. A new track gets its manifest written at once, so
    /// the directory is never left with chunks and no manifest.
    pub fn open(
        track_dir: &Path,
        track: TrackId,
        role: &str,
        origin: TrackOrigin,
    ) -> std::io::Result<ChunkWriter> {
        std::fs::create_dir_all(track_dir)?;
        let manifest = match ChunkManifest::load(track_dir) {
            Ok(Some(m)) => m,
            Ok(None) => {
                let m = ChunkManifest::new(track, role, origin);
                m.save(track_dir)?;
                m
            }
            Err(e) => return Err(std::io::Error::other(e)),
        };
        let next_seq = manifest.max_seq().map_or(0, |s| s + 1);
        let position = manifest
            .chunks
            .iter()
            .map(|c| c.start_sample + c.len_samples)
            .chain(manifest.gaps.iter().map(|g| g.to_sample))
            .max()
            .unwrap_or(0);
        Ok(ChunkWriter {
            track_dir: track_dir.to_path_buf(),
            manifest,
            queue: VecDeque::new(),
            position,
            next_seq,
            events: Vec::new(),
            pending: VecDeque::new(),
            disk_full: false,
        })
    }

    pub fn manifest(&self) -> &ChunkManifest {
        &self.manifest
    }
    pub fn queued_samples(&self) -> usize {
        self.queue.len()
    }
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Accept samples from the capture callback. Never blocks; overflow drops and records a gap.
    pub fn push(&mut self, samples: &[i16]) {
        let room = QUEUE_CAP_SAMPLES.saturating_sub(self.queue.len());
        let accepted = samples.len().min(room);
        self.queue.extend(samples[..accepted].iter().copied());
        let dropped = (samples.len() - accepted) as u64;
        if dropped > 0 {
            let reason = if self.disk_full {
                DegradedReason::DiskFull
            } else {
                DegradedReason::DiskBackpressure
            };
            self.record_drop(dropped, reason);
        }
    }

    fn record_drop(&mut self, dropped: u64, reason: DegradedReason) {
        match self.pending.back_mut() {
            Some(last) if last.queue_offset == self.queue.len() => {
                last.dropped += dropped;
                let gap_index = last.gap_index;
                self.manifest.gaps[gap_index].to_sample += dropped;
            }
            _ => {
                // the gap sits after everything queued, and after every gap already pending
                let from = self.position
                    + self.queue.len() as u64
                    + self.pending.iter().map(|p| p.dropped).sum::<u64>();
                self.manifest.gaps.push(ma_core_types::timeline::Gap {
                    from_sample: from,
                    to_sample: from + dropped,
                    reason: GapReason::CaptureInterrupted,
                });
                self.pending.push_back(PendingGap {
                    queue_offset: self.queue.len(),
                    dropped,
                    gap_index: self.manifest.gaps.len() - 1,
                });
            }
        }
        self.events.push(WriterEvent::Degraded {
            reason,
            dropped_samples: dropped,
        });
    }

    /// How many queued samples may go into the next chunk before the first pending gap.
    fn next_run(&self) -> usize {
        match self.pending.front() {
            Some(p) => p.queue_offset.min(self.queue.len()),
            None => self.queue.len(),
        }
    }

    /// When the queue has been consumed up to the first pending gap, the position jumps over it.
    fn apply_gaps_reached(&mut self) {
        while let Some(p) = self.pending.front() {
            if p.queue_offset == 0 {
                self.position += p.dropped;
                self.pending.pop_front();
            } else {
                break;
            }
        }
    }

    /// Write every complete chunk the queue holds. A stalled or full filesystem leaves samples
    /// queued. A chunk ends early at a recorded gap, so positions after the gap stay exact.
    pub fn drain(&mut self, fs: &mut dyn ChunkFs) -> Result<(), FsError> {
        loop {
            self.apply_gaps_reached();
            let run = self.next_run();
            let at_gap = self.pending.front().is_some_and(|p| p.queue_offset == run);
            if run == 0 || (run < CHUNK_SAMPLES && !at_gap) {
                return Ok(());
            }
            let n = run.min(CHUNK_SAMPLES);
            self.write_front(fs, n)?;
        }
    }

    /// Flush whatever remains at session end.
    pub fn finish(&mut self, fs: &mut dyn ChunkFs) -> Result<(), FsError> {
        self.drain(fs)?;
        loop {
            self.apply_gaps_reached();
            let run = self.next_run();
            if run == 0 {
                break;
            }
            self.write_front(fs, run.min(CHUNK_SAMPLES))?;
        }
        self.apply_gaps_reached();
        Ok(())
    }

    fn write_front(&mut self, fs: &mut dyn ChunkFs, n: usize) -> Result<(), FsError> {
        let samples: Vec<i16> = self.queue.iter().take(n).copied().collect();
        self.write_chunk(fs, &samples)?;
        self.queue.drain(..n);
        for p in &mut self.pending {
            p.queue_offset -= n;
        }
        Ok(())
    }

    fn write_chunk(&mut self, fs: &mut dyn ChunkFs, samples: &[i16]) -> Result<(), FsError> {
        let origin = &self.manifest.origin;
        let bytes = wav::encode(origin.sample_rate, origin.channels, samples);
        let seq = self.next_seq;
        let file = ChunkManifest::chunk_file(seq);
        let part = self.track_dir.join(format!("{file}.part"));
        let final_path = self.track_dir.join(&file);
        let start_sample = self.position;
        // durability order: part → flush → rename → manifest record → fsync; nothing is mutated
        // before the filesystem accepted the bytes, so a stall leaves every position untouched
        match fs.write_part(&part, &bytes) {
            Ok(()) => self.disk_full = false,
            Err(FsError::DiskFull) => {
                self.disk_full = true;
                return Err(FsError::DiskFull);
            }
            Err(e) => return Err(e),
        }
        fs.rename_in(&part, &final_path)?;
        self.manifest.chunks.push(ChunkRecord {
            seq,
            file,
            start_sample,
            len_samples: samples.len() as u64,
        });
        fs.save_manifest(&self.track_dir, &self.manifest)?;
        self.next_seq += 1;
        self.position = start_sample + samples.len() as u64;
        self.events.push(WriterEvent::ChunkDurable {
            seq,
            samples: samples.len() as u64,
        });
        Ok(())
    }

    pub fn track_id(&self) -> TrackId {
        self.manifest.track
    }

    /// A device format or endpoint change: finish this track and open its successor — a new
    /// track identifier, directory, origin and sample space starting at 0 — so old positions are
    /// never reinterpreted at the new rate and every origin has exactly one track.
    pub fn open_successor(
        &mut self,
        fs: &mut dyn ChunkFs,
        tracks_root: &Path,
        successor: TrackId,
        origin: TrackOrigin,
        now_ms: u64,
    ) -> Result<ChunkWriter, FsError> {
        self.finish(fs)?;
        self.manifest.successor = Some(successor);
        self.manifest.events.push(ManifestEvent {
            kind: ManifestEventKind::SuccessorOpened,
            at_ms: now_ms,
            seq: None,
            file: Some(successor.to_string()),
            samples: Some(self.position),
        });
        fs.save_manifest(&self.track_dir, &self.manifest)?;
        self.events.push(WriterEvent::SuccessorOpened { successor });
        let dir = tracks_root.join(successor.to_string());
        let mut next = ChunkWriter::open(&dir, successor, &self.manifest.role, origin)
            .map_err(|_| FsError::Io)?;
        next.manifest.predecessor = Some(self.manifest.track);
        fs.save_manifest(&next.track_dir, &next.manifest)?;
        Ok(next)
    }
}
