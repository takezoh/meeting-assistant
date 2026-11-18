//! Calculations for mixing multiple audio inputs into one. No side effects.
//!
//! For each input, the capture side puts arriving sound into a [`SourceBuffer`], and the writing side takes one frame
//! interval's worth at a time and adds it. Capture and writing advance on separate clocks. The buffer keeps just
//! enough queued that a mix never runs short, and no more, because whatever is queued is how far the sound lags the
//! picture:
//! - When writing starts, what was captured while the recording was being prepared is dropped.
//! - Mixing starts once one mix, the largest packet seen so far and a margin are queued, so that sound arriving in
//!   packets and a little late does not cause dropouts.
//! - The lowest amount queued before a mix is held near one mix plus the margin by dropping or repeating single
//!   frames. This absorbs the drift between the capture clock and the writing clock without audible jumps. The lowest
//!   amount is only seen at mixes, so it can be lower between them by up to a packet or a mix, whichever is smaller;
//!   that much is kept on top.
//! - If far too much accumulates (the writing side stalled), the excess is dropped at once.
//! - On underrun, the missing part is silence, and mixing waits until it can start again.

use std::collections::VecDeque;

/// Values that determine how absorption works. Units are sample counts (all channels combined).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPolicy {
    /// Samples in one frame: the unit that is dropped or repeated.
    pub frame: usize,
    /// Kept beyond what one mix takes, against jitter in arrival.
    pub margin: usize,
    /// How far the lowest amount queued may stray from its target before frames are dropped or repeated.
    pub tolerance: usize,
    /// How far above its target the lowest amount may rise before the excess is dropped at once.
    pub max_excess: usize,
    /// How much is mixed between two looks at the lowest amount queued.
    pub window: usize,
    /// Never buffer more than this, even if the writing side is stalled.
    pub hard_limit: usize,
}

impl BufferPolicy {
    /// Derives the default values from the sample rate and channel count (a margin of 20 ms, a tolerance of 5 ms,
    /// a look every second, the excess dropped at once above 200 ms, a cap of 2 s).
    pub fn for_format(sample_rate: u32, channels: usize) -> Self {
        let per_millisecond = |milliseconds: usize| sample_rate as usize * channels * milliseconds / 1000;
        Self {
            frame: channels,
            margin: per_millisecond(20),
            tolerance: per_millisecond(5),
            max_excess: per_millisecond(200),
            window: per_millisecond(1000),
            hard_limit: per_millisecond(2000),
        }
    }
}

/// What the absorption did over the life of one buffer, for the log. Units are sample counts (all channels
/// combined), like [`BufferPolicy`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BufferReport {
    /// How much was dropped when writing started, because it had been captured before.
    pub dropped_at_start: usize,
    /// How much was queued when mixing first took sound from this input: the delay its sound started with.
    pub queued_at_start: Option<usize>,
    /// Single frames dropped and repeated to follow the drift between the clocks.
    pub frames_dropped: usize,
    pub frames_repeated: usize,
    /// How often an excess was dropped at once, and how much was dropped in total.
    pub catch_ups: u32,
    pub caught_up: usize,
    /// How often mixing ran short, and how much was missing at those moments. The silence written afterwards while
    /// refilling is not counted, because it cannot be told apart from an input that has nothing to play.
    pub underruns: u32,
    pub missing: usize,
    /// How much was dropped because the writing side did not take it in time.
    pub overflow: usize,
}

/// Sound of one input that has not been mixed yet.
pub struct SourceBuffer {
    policy: BufferPolicy,
    queue: VecDeque<i16>,
    primed: bool,
    /// Writing has started; the next mix drops what was captured before.
    starting: bool,
    /// The most that has arrived at once in the current and in the previous window: how unevenly this input
    /// delivers now. One unusually large packet is forgotten after two windows.
    packet_now: usize,
    packet_before: usize,
    /// The lowest amount queued before a mix in the current window, and how much has been mixed in it.
    lowest: usize,
    mixed: usize,
    /// Frames still to drop (positive) or repeat (negative), one per mix.
    correction: isize,
    report: BufferReport,
}

impl SourceBuffer {
    pub fn new(policy: BufferPolicy) -> Self {
        Self {
            policy,
            queue: VecDeque::new(),
            primed: false,
            starting: false,
            packet_now: 0,
            packet_before: 0,
            lowest: usize::MAX,
            mixed: 0,
            correction: 0,
            report: BufferReport::default(),
        }
    }

    /// Adds arriving sound.
    pub fn push(&mut self, samples: &[i16]) {
        self.queue.extend(samples.iter().copied());
        self.arrived(samples.len());
    }

    /// When what arrived was silence.
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "only the Windows capture reports silent packets")
    )]
    pub fn push_silence(&mut self, samples: usize) {
        self.queue.extend(std::iter::repeat_n(0, samples));
        self.arrived(samples);
    }

    fn arrived(&mut self, samples: usize) {
        self.packet_now = self.packet_now.max(samples);
        let overflow = self.queue.len().saturating_sub(self.policy.hard_limit);
        self.queue.drain(..overflow);
        self.report.overflow += overflow;
    }

    /// Writing starts now. The next mix keeps only as much as it needs to start, so that the first sound mixed is
    /// current rather than what was captured while the output and the first picture were being prepared.
    pub fn start_writing(&mut self) {
        self.starting = true;
    }

    /// What the absorption has done so far.
    #[cfg_attr(not(windows), allow(dead_code, reason = "only the Windows capture logs the report"))]
    pub fn report(&self) -> BufferReport {
        self.report
    }

    /// How unevenly this input delivers, as the largest recent packet. Capped, so that starting never needs more than
    /// the buffer may hold.
    fn packet(&self) -> usize {
        self.packet_now.max(self.packet_before).min(self.policy.max_excess)
    }

    /// Adds this input's sound to `out`. A shortfall is treated as silence (`out` is left unchanged).
    pub fn mix_into(&mut self, out: &mut [i16]) {
        // The lowest amount queued before a mix should be one mix plus the margin, plus what a look at mixes alone can
        // miss. Starting with one packet more keeps it there whichever moment of the packet rhythm mixing starts at.
        let packet = self.packet();
        let target = out.len() + self.policy.margin + packet.min(out.len());
        let start = target + packet;
        if std::mem::take(&mut self.starting) {
            let backlog = self.queue.len().saturating_sub(start);
            self.queue.drain(..backlog);
            self.report.dropped_at_start += backlog;
        }
        if !self.primed {
            if self.queue.len() < start {
                return;
            }
            self.primed = true;
            self.report.queued_at_start.get_or_insert(self.queue.len());
            self.lowest = usize::MAX;
            self.mixed = 0;
            self.correction = 0;
        }

        self.follow_drift(target, out.len());

        let available = self.queue.len().min(out.len());
        for (target, sample) in out.iter_mut().zip(self.queue.drain(..available)) {
            *target = target.saturating_add(sample);
        }
        if available < out.len() {
            // Underrun. When more arrives, refill before resuming.
            self.primed = false;
            self.report.underruns += 1;
            self.report.missing += out.len() - available;
        }
    }

    /// Looks at the lowest amount queued once per window and moves it towards `target`: a large excess at once,
    /// a small difference one frame per mix.
    fn follow_drift(&mut self, target: usize, taking: usize) {
        let frame = self.policy.frame.max(1);
        self.lowest = self.lowest.min(self.queue.len());
        self.mixed += taking;
        if self.mixed >= self.policy.window {
            let deviation = self.lowest as isize - target as isize;
            if deviation > self.policy.max_excess as isize {
                let excess = deviation as usize / frame * frame;
                self.queue.drain(..excess);
                self.report.catch_ups += 1;
                self.report.caught_up += excess;
                self.correction = 0;
            } else if deviation.unsigned_abs() > self.policy.tolerance {
                // Half the difference per window, so that one unusual window does not swing it the other way.
                self.correction = deviation / 2 / frame as isize;
            } else {
                self.correction = 0;
            }
            self.lowest = usize::MAX;
            self.mixed = 0;
            self.packet_before = std::mem::take(&mut self.packet_now);
        }

        if self.correction > 0 && self.queue.len() >= taking + frame {
            self.queue.drain(..frame);
            self.correction -= 1;
            self.report.frames_dropped += 1;
        } else if self.correction < 0 && self.queue.len() >= frame {
            // Each sample pushed to the front moves the frame's earlier samples up by one.
            for _ in 0..frame {
                let sample = self.queue[frame - 1];
                self.queue.push_front(sample);
            }
            self.correction += 1;
            self.report.frames_repeated += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One "millisecond" is 10 samples of one channel, so that the numbers stay small.
    const POLICY: BufferPolicy = BufferPolicy {
        frame: 1,
        margin: 4,
        tolerance: 2,
        max_excess: 40,
        window: 100,
        hard_limit: 200,
    };

    fn mixed(buffer: &mut SourceBuffer, samples: usize) -> Vec<i16> {
        let mut out = vec![0; samples];
        buffer.mix_into(&mut out);
        out
    }

    fn counting(from: i16, count: usize) -> Vec<i16> {
        (0..count as i16).map(|i| from + i).collect()
    }

    /// Delivers `samples` in packets of `packet`.
    fn push_in(buffer: &mut SourceBuffer, samples: &[i16], packet: usize) {
        for chunk in samples.chunks(packet) {
            buffer.push(chunk);
        }
    }

    #[test]
    fn default_values_scale_with_sample_rate_and_channel_count() {
        let policy = BufferPolicy::for_format(48_000, 2);

        assert_eq!(
            policy,
            BufferPolicy {
                frame: 2,
                margin: 1_920,
                tolerance: 480,
                max_excess: 19_200,
                window: 96_000,
                hard_limit: 192_000
            }
        );
    }

    #[test]
    fn mixing_starts_once_one_mix_the_largest_packet_and_the_margin_are_queued() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&counting(1, 3));
        buffer.push(&counting(4, 3));
        buffer.push(&counting(7, 2));

        assert_eq!(mixed(&mut buffer, 2), [0, 0], "2 + 4 + 2 + 3 = 11 are needed");
        assert_eq!(buffer.queue.len(), 8, "nothing is taken until then");

        buffer.push(&[9, 10, 11]);
        assert_eq!(mixed(&mut buffer, 2), [1, 2]);
        assert_eq!(buffer.report().queued_at_start, Some(11));
    }

    #[test]
    fn starting_to_write_drops_what_was_captured_before() {
        let mut buffer = SourceBuffer::new(POLICY);
        for start in (1..=30).step_by(3) {
            buffer.push(&counting(start, 3));
        }

        buffer.start_writing();

        // One mix of 2, the margin of 4, what a look can miss (2) and a packet of 3 are kept: the newest 11 of 30.
        assert_eq!(mixed(&mut buffer, 2), [20, 21]);
        assert_eq!(buffer.report().dropped_at_start, 19);
        assert_eq!(buffer.report().queued_at_start, Some(11));
    }

    #[test]
    fn starting_to_write_with_a_short_backlog_still_waits_to_fill() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[1, 2]);

        buffer.start_writing();

        assert_eq!(mixed(&mut buffer, 2), [0, 0], "not enough to start yet");
        push_in(&mut buffer, &counting(3, 8), 2);
        assert_eq!(mixed(&mut buffer, 2), [1, 2]);
        assert_eq!(buffer.report().dropped_at_start, 0);
    }

    #[test]
    fn adds_to_existing_sound_and_clamps_on_overflow() {
        let mut buffer = SourceBuffer::new(POLICY);
        push_in(&mut buffer, &[i16::MAX, i16::MIN, 1, 1, 0, 0, 0, 0], 1);
        let mut out = vec![1, -1];

        buffer.mix_into(&mut out);

        assert_eq!(out, [i16::MAX, i16::MIN]);
    }

    #[test]
    fn treats_shortfall_as_silence_and_refills_before_resuming() {
        let mut buffer = SourceBuffer::new(POLICY);
        push_in(&mut buffer, &counting(1, 8), 1);

        assert_eq!(mixed(&mut buffer, 2), [1, 2]);
        assert_eq!(mixed(&mut buffer, 8), [3, 4, 5, 6, 7, 8, 0, 0]);

        push_in(&mut buffer, &counting(9, 6), 1);
        assert_eq!(mixed(&mut buffer, 2), [0, 0], "silence while refilling");
        push_in(&mut buffer, &[15, 16], 1);
        assert_eq!(mixed(&mut buffer, 2), [9, 10]);
        assert_eq!((buffer.report().underruns, buffer.report().missing), (1, 2));
    }

    #[test]
    fn does_not_buffer_beyond_the_cap_even_if_the_writing_side_stalls() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&counting(1, 150));
        buffer.push_silence(100);

        assert_eq!(buffer.queue.len(), 200);
        assert_eq!(buffer.report().overflow, 50);
    }

    #[test]
    fn silence_that_arrived_also_advances_time() {
        let mut buffer = SourceBuffer::new(POLICY);
        for _ in 0..4 {
            buffer.push_silence(1);
        }
        push_in(&mut buffer, &[5, 6, 7, 8, 9, 10], 1);

        let taken: Vec<i16> = (0..3).flat_map(|_| mixed(&mut buffer, 2)).collect();
        assert_eq!(taken, [0, 0, 0, 0, 5, 6]);
    }

    #[test]
    fn a_frame_is_dropped_or_repeated_as_a_whole() {
        let policy = BufferPolicy {
            frame: 2,
            window: 2,
            tolerance: 0,
            ..POLICY
        };
        // Too much queued: the oldest frame goes.
        let mut buffer = SourceBuffer::new(policy);
        push_in(&mut buffer, &counting(1, 20), 2);
        assert_eq!(mixed(&mut buffer, 2), [3, 4], "frame [1, 2] was dropped");
        assert_eq!(buffer.report().frames_dropped, 1);

        // Too little queued: the oldest frame is played twice rather than running short.
        let mut buffer = SourceBuffer::new(policy);
        push_in(&mut buffer, &counting(1, 10), 2);
        let taken: Vec<Vec<i16>> = (0..6).map(|_| mixed(&mut buffer, 2)).collect();
        assert_eq!(taken, [[1, 2], [3, 4], [5, 6], [7, 8], [9, 10], [9, 10]]);
        assert!(buffer.report().frames_repeated >= 1);
        assert_eq!(buffer.report().underruns, 0);
    }

    /// What one input does over `seconds` of simulated time at 48 kHz stereo: it delivers `packet_ms` of sound
    /// every `packet_ms`, on a clock `drift_ppm` faster than the writing side, while 30 mixes a second take
    /// 1600 frames each. Returns the report and the delay (amount queued before each mix) in milliseconds, from the
    /// second minute on.
    fn simulate(packet_ms: u64, drift_ppm: i64, seconds: u64) -> (BufferReport, Vec<f64>) {
        let policy = BufferPolicy::for_format(48_000, 2);
        let mut buffer = SourceBuffer::new(policy);
        let packet = vec![1i16; (96 * packet_ms) as usize];
        let packet_interval = packet_ms as f64 * 1e6 / (1.0 + drift_ppm as f64 * 1e-6);
        let mix_interval = 1e9 / 30.0;
        // Some sound arrives while the recording is being prepared.
        let (mut next_packet, mut next_mix) = (0.0, 300e6);
        buffer.start_writing();
        let mut delays = Vec::new();
        while next_mix < seconds as f64 * 1e9 {
            if next_packet <= next_mix {
                buffer.push(&packet);
                next_packet += packet_interval;
            } else {
                if next_mix >= 60e9 {
                    delays.push(buffer.queue.len() as f64 / 96.0);
                }
                let mut out = vec![0i16; 3200];
                buffer.mix_into(&mut out);
                next_mix += mix_interval;
            }
        }
        (buffer.report(), delays)
    }

    #[test]
    fn packets_of_10_ms_on_a_drifting_clock_neither_run_short_nor_pile_up() {
        for drift in [-200, -20, 0, 20, 200] {
            let (report, delays) = simulate(10, drift, 1200);
            assert_eq!(report.underruns, 0, "drift {drift} ppm: {report:?}");
            assert_eq!(report.catch_ups, 0, "drift {drift} ppm: {report:?}");
            let (low, high) = delays
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), &d| (l.min(d), h.max(d)));
            assert!(
                low >= 45.0 && high <= 90.0,
                "drift {drift} ppm: delay {low:.1} to {high:.1} ms"
            );
        }
    }

    #[test]
    fn packets_of_100_ms_on_a_drifting_clock_neither_run_short_nor_pile_up() {
        for drift in [-200, 0, 200] {
            let (report, delays) = simulate(100, drift, 1200);
            assert_eq!(report.underruns, 0, "drift {drift} ppm: {report:?}");
            assert_eq!(report.catch_ups, 0, "drift {drift} ppm: {report:?}");
            let high = delays.iter().cloned().fold(f64::MIN, f64::max);
            assert!(high <= 240.0, "drift {drift} ppm: delay up to {high:.1} ms");
        }
    }

    #[test]
    fn one_unusually_large_packet_is_forgotten_after_two_windows() {
        let mut buffer = SourceBuffer::new(POLICY);
        push_in(&mut buffer, &counting(1, 12), 3);
        buffer.push(&[0; 30]);
        // While it is recent, starting again after an underrun needs it: 2 + 4 + 2 + 30.
        mixed(&mut buffer, 2);
        assert_eq!(buffer.packet(), 30);
        // Three windows of 100 mixed later, with packets of 2, only those count.
        for _ in 0..150 {
            buffer.push(&[1, 1]);
            mixed(&mut buffer, 2);
        }
        assert_eq!(buffer.packet(), 2);
    }

    #[test]
    fn a_huge_packet_does_not_keep_mixing_from_ever_starting() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&counting(1, 120));

        // The packet counts as at most 40 (the largest excess), so 2 + 4 + 2 + 40 are needed, not 2 + 4 + 2 + 120.
        assert_eq!(mixed(&mut buffer, 2), [1, 2]);
    }

    #[test]
    fn a_stall_of_the_writing_side_is_dropped_at_once_afterwards() {
        let mut buffer = SourceBuffer::new(POLICY);
        // Packets of 3, mixes of 3 (window of 100 = 33 mixes).
        push_in(&mut buffer, &counting(1, 9), 3);
        for _ in 0..40 {
            buffer.push(&[1, 1, 1]);
            mixed(&mut buffer, 3);
        }
        // The writing side stops for 60 samples' worth, then goes on.
        for _ in 0..20 {
            buffer.push(&[1, 1, 1]);
        }
        for _ in 0..80 {
            buffer.push(&[1, 1, 1]);
            mixed(&mut buffer, 3);
        }

        let report = buffer.report();
        assert_eq!(report.catch_ups, 1, "{report:?}");
        assert_eq!(report.underruns, 0, "{report:?}");
        assert!(
            buffer.queue.len() <= 3 + 4 + 3 + 3 + 2,
            "back near its target: {}",
            buffer.queue.len()
        );
    }
}
