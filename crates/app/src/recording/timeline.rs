//! How recording timestamps advance. No side effects.
//!
//! Writes video one picture at a time at a fixed interval, and after each one writes
//! the audio for the span that picture is on screen.
//! This way video and audio times both come from the same clock, so drift does not accumulate.

use std::time::Duration;

/// Number of time units per second used for writing (one unit is 100 ns).
pub const TICKS_PER_SECOND: u64 = 10_000_000;
const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// Write time and duration (in 100 ns units) of one picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub video_time: i64,
    pub video_duration: i64,
    pub audio_time: i64,
    pub audio_duration: i64,
    /// Number of audio frames belonging to this picture.
    pub audio_frames: usize,
}

pub struct Timeline {
    framerate: u64,
    sample_rate: u64,
    /// Index of the next picture to write.
    frame_index: u64,
    /// Number of audio frames written so far.
    audio_frames: u64,
}

impl Timeline {
    pub fn new(framerate: u32, sample_rate: u32) -> Self {
        Self {
            framerate: u64::from(framerate.max(1)),
            sample_rate: u64::from(sample_rate),
            frame_index: 0,
            audio_frames: 0,
        }
    }

    /// Time at which the next picture should be written (elapsed since the recording started).
    pub fn next_due(&self) -> Duration {
        Duration::from_nanos(self.frame_index * NANOS_PER_SECOND / self.framerate)
    }

    /// If writing has fallen behind by more than `threshold` (e.g. after resuming from sleep),
    /// jump to the current time instead of trying to catch up. Returns true if it jumped.
    /// Neither video nor audio is written for the skipped span.
    pub fn skip_ahead_if_late(&mut self, elapsed: Duration, threshold: Duration) -> bool {
        if elapsed.saturating_sub(self.next_due()) <= threshold {
            return false;
        }
        self.frame_index = elapsed.as_nanos() as u64 * self.framerate / NANOS_PER_SECOND;
        self.audio_frames = self.frame_index * self.sample_rate / self.framerate;
        true
    }

    /// Advances by one picture and returns the video and audio times.
    pub fn step(&mut self) -> Step {
        let video_time = self.frame_index * TICKS_PER_SECOND / self.framerate;
        let video_end = (self.frame_index + 1) * TICKS_PER_SECOND / self.framerate;
        // Write the audio up to the point where this picture stops being displayed.
        let audio_target = (self.frame_index + 1) * self.sample_rate / self.framerate;
        let audio_time = self.audio_frames * TICKS_PER_SECOND / self.sample_rate;
        let audio_end = audio_target * TICKS_PER_SECOND / self.sample_rate;

        let step = Step {
            video_time: video_time as i64,
            video_duration: (video_end - video_time) as i64,
            audio_time: audio_time as i64,
            audio_duration: (audio_end - audio_time) as i64,
            audio_frames: (audio_target - self.audio_frames) as usize,
        };
        self.frame_index += 1;
        self.audio_frames = audio_target;
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Advances by `seconds`; returns the video end, the audio end, and the total number of audio frames.
    fn run(framerate: u32, sample_rate: u32, seconds: u32) -> (i64, i64, usize) {
        let mut timeline = Timeline::new(framerate, sample_rate);
        let mut last = None;
        let mut frames = 0;
        for _ in 0..framerate * seconds {
            let step = timeline.step();
            if let Some(Step {
                video_time,
                video_duration,
                audio_time,
                audio_duration,
                ..
            }) = last
            {
                assert_eq!(
                    step.video_time,
                    video_time + video_duration,
                    "no gap or overlap in video"
                );
                assert_eq!(
                    step.audio_time,
                    audio_time + audio_duration,
                    "no gap or overlap in audio"
                );
            }
            frames += step.audio_frames;
            last = Some(step);
        }
        let last = last.unwrap();
        (
            last.video_time + last.video_duration,
            last.audio_time + last.audio_duration,
            frames,
        )
    }

    #[test]
    fn first_picture_and_audio_start_at_time_0() {
        let step = Timeline::new(30, 48_000).step();

        assert_eq!((step.video_time, step.audio_time), (0, 0));
        assert_eq!(step.video_duration, 333_333);
        assert_eq!(step.audio_frames, 1_600);
    }

    #[test]
    fn video_and_audio_advance_by_the_same_length() {
        for (framerate, sample_rate) in [(30, 48_000), (60, 48_000), (15, 44_100), (7, 48_000), (1, 48_000)] {
            let (video_end, audio_end, frames) = run(framerate, sample_rate, 3);

            assert_eq!(video_end, 30_000_000, "video at {framerate}fps");
            assert_eq!(audio_end, 30_000_000, "audio at {framerate}fps");
            assert_eq!(frames, sample_rate as usize * 3);
        }
    }

    #[test]
    fn audio_frame_total_matches_even_for_non_divisible_framerates() {
        let mut timeline = Timeline::new(7, 48_000);
        let frames: Vec<usize> = (0..7).map(|_| timeline.step().audio_frames).collect();

        assert_eq!(frames.iter().sum::<usize>(), 48_000);
        assert!(frames.iter().all(|&n| n == 6_857 || n == 6_858), "{frames:?}");
    }

    #[test]
    fn next_write_time_is_determined_by_picture_count() {
        let mut timeline = Timeline::new(30, 48_000);
        assert_eq!(timeline.next_due(), Duration::ZERO);

        for _ in 0..30 {
            timeline.step();
        }
        assert_eq!(timeline.next_due(), Duration::from_secs(1));
    }

    #[test]
    fn does_not_skip_for_a_small_delay() {
        let mut timeline = Timeline::new(30, 48_000);

        assert!(!timeline.skip_ahead_if_late(Duration::from_secs(2), Duration::from_secs(2)));
        assert_eq!(timeline.step().video_time, 0);
    }

    #[test]
    fn jumps_to_current_time_after_a_large_delay_keeping_video_and_audio_aligned() {
        let mut timeline = Timeline::new(30, 48_000);
        timeline.step();

        assert!(timeline.skip_ahead_if_late(Duration::from_secs(60), Duration::from_secs(2)));
        let step = timeline.step();

        assert_eq!(step.video_time, 600_000_000);
        assert_eq!(step.audio_time, 600_000_000);
        assert_eq!(
            step.audio_frames, 1_600,
            "audio for the skipped span is not written in one lump"
        );
        assert!(!timeline.skip_ahead_if_late(Duration::from_secs(60), Duration::from_secs(2)));
    }
}
