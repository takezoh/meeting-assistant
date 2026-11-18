//! Calculations for mixing multiple audio inputs into one. No side effects.
//!
//! For each input, the capture side puts arriving sound into a [`SourceBuffer`],
//! and the writing side takes what it needs and adds it.
//! Capture and writing advance on separate clocks, so jitter and drift are absorbed as follows.
//! - Buffer a little before starting to mix (so jitter does not cause dropouts).
//! - On underrun, treat the missing part as silence, and refill before resuming when more arrives.
//! - If too much accumulates, drop the oldest part to catch up (so delay does not build up).

use std::collections::VecDeque;

/// Values that determine how absorption works. Units are sample counts (all channels combined).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPolicy {
    /// Amount to buffer before starting to mix.
    pub prime: usize,
    /// When more than this accumulates, drop down to `prime`.
    pub max_queued: usize,
    /// Never buffer more than this, even if the writing side is stalled.
    pub hard_limit: usize,
}

impl BufferPolicy {
    /// Derives the default values from the sample rate and channel count
    /// (buffer 40 ms, catch up at 500 ms, cap at 2 s).
    pub fn for_format(sample_rate: u32, channels: usize) -> Self {
        let per_millisecond = |milliseconds: usize| sample_rate as usize * channels * milliseconds / 1000;
        Self {
            prime: per_millisecond(40),
            max_queued: per_millisecond(500),
            hard_limit: per_millisecond(2000),
        }
    }
}

/// Sound of one input that has not been mixed yet.
pub struct SourceBuffer {
    policy: BufferPolicy,
    queue: VecDeque<i16>,
    primed: bool,
}

impl SourceBuffer {
    pub fn new(policy: BufferPolicy) -> Self {
        Self {
            policy,
            queue: VecDeque::new(),
            primed: false,
        }
    }

    /// Adds arriving sound.
    pub fn push(&mut self, samples: &[i16]) {
        self.queue.extend(samples.iter().copied());
        self.enforce_hard_limit();
    }

    /// When what arrived was silence.
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "only the Windows capture reports silent packets")
    )]
    pub fn push_silence(&mut self, samples: usize) {
        self.queue.extend(std::iter::repeat_n(0, samples));
        self.enforce_hard_limit();
    }

    fn enforce_hard_limit(&mut self) {
        let overflow = self.queue.len().saturating_sub(self.policy.hard_limit);
        self.queue.drain(..overflow);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.queue.len()
    }

    /// Adds this input's sound to `out`. A shortfall is treated as silence (`out` is left unchanged).
    pub fn mix_into(&mut self, out: &mut [i16]) {
        if !self.primed {
            if self.queue.len() < self.policy.prime {
                return;
            }
            self.primed = true;
        }
        if self.queue.len() > self.policy.max_queued {
            let excess = self.queue.len() - self.policy.prime;
            self.queue.drain(..excess);
        }

        let available = self.queue.len().min(out.len());
        for (target, sample) in out.iter_mut().zip(self.queue.drain(..available)) {
            *target = target.saturating_add(sample);
        }
        if available < out.len() {
            // Underrun. When more arrives, refill before resuming.
            self.primed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: BufferPolicy = BufferPolicy {
        prime: 4,
        max_queued: 10,
        hard_limit: 20,
    };

    fn mixed(buffer: &mut SourceBuffer, samples: usize) -> Vec<i16> {
        let mut out = vec![0; samples];
        buffer.mix_into(&mut out);
        out
    }

    #[test]
    fn default_values_scale_with_sample_rate_and_channel_count() {
        let policy = BufferPolicy::for_format(48_000, 2);

        assert_eq!(
            policy,
            BufferPolicy {
                prime: 3_840,
                max_queued: 48_000,
                hard_limit: 192_000
            }
        );
    }

    #[test]
    fn does_not_start_mixing_until_enough_has_accumulated() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[1, 2, 3]);

        assert_eq!(mixed(&mut buffer, 2), [0, 0]);
        assert_eq!(buffer.len(), 3, "nothing is taken until enough has accumulated");
    }

    #[test]
    fn takes_in_arrival_order_once_accumulated() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[1, 2, 3, 4, 5]);

        assert_eq!(mixed(&mut buffer, 2), [1, 2]);
        assert_eq!(mixed(&mut buffer, 2), [3, 4]);
    }

    #[test]
    fn adds_to_existing_sound() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[1, 2, 3, 4]);
        let mut out = vec![10, 20, 30, 40];

        buffer.mix_into(&mut out);

        assert_eq!(out, [11, 22, 33, 44]);
    }

    #[test]
    fn clamps_to_upper_and_lower_limits_on_overflow() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[i16::MAX, i16::MIN, 1, 1]);
        let mut out = vec![1, -1, 0, 0];

        buffer.mix_into(&mut out);

        assert_eq!(out, [i16::MAX, i16::MIN, 1, 1]);
    }

    #[test]
    fn treats_shortfall_as_silence_and_refills_before_resuming() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&[1, 2, 3, 4]);

        assert_eq!(mixed(&mut buffer, 6), [1, 2, 3, 4, 0, 0]);

        buffer.push(&[5, 6]);
        assert_eq!(mixed(&mut buffer, 2), [0, 0], "silence while buffering");
        buffer.push(&[7, 8]);
        assert_eq!(mixed(&mut buffer, 2), [5, 6]);
    }

    #[test]
    fn drops_the_oldest_part_to_catch_up_when_too_much_accumulates() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&(1..=12).collect::<Vec<i16>>());

        assert_eq!(
            mixed(&mut buffer, 2),
            [9, 10],
            "keep only the amount to buffer and drop the older part"
        );
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn does_not_buffer_beyond_the_cap_even_if_the_writing_side_stalls() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push(&(1..=15).collect::<Vec<i16>>());
        buffer.push_silence(10);

        assert_eq!(buffer.len(), 20);
    }

    #[test]
    fn silence_that_arrived_also_advances_time() {
        let mut buffer = SourceBuffer::new(POLICY);
        buffer.push_silence(4);
        buffer.push(&[5, 6]);

        assert_eq!(mixed(&mut buffer, 6), [0, 0, 0, 0, 5, 6]);
    }
}
