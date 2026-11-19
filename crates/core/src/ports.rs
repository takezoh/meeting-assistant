//! Small ports that the recording coordinator relies on for the outside world.
//!
//! The ports for detection signals ([`crate::SignalSource`]) and recording ([`crate::Recorder`]) sit next to the
//! data they handle.
//! This module holds the rest: the clock, the output folder, and waiting.

use std::path::PathBuf;
use std::time::Duration;

/// The user's local date and time. Used in recording file names and in what is written into a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// How far local time is ahead of UTC, in minutes.
    pub utc_offset_minutes: i32,
}

impl LocalDateTime {
    /// The time in ISO 8601 with its offset from UTC, as `2026-11-19T19:01:56+09:00`.
    pub fn iso8601(&self) -> String {
        let sign = if self.utc_offset_minutes < 0 { '-' } else { '+' };
        let offset = self.utc_offset_minutes.unsigned_abs();
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            offset / 60,
            offset % 60
        )
    }
}

/// Returns the current date and time.
pub trait Clock {
    fn now(&self) -> LocalDateTime;
}

/// The folder where recordings are saved.
pub trait OutputFolder {
    /// Makes the folder (creating it if missing) ready for writing.
    fn prepare(&mut self) -> anyhow::Result<()>;

    /// Whether a file with that name already exists.
    fn contains(&self, file_name: &str) -> bool;

    /// The location of the file with that name.
    fn path_of(&self, file_name: &str) -> PathBuf;
}

/// Pauses before a retry.
pub trait Pause {
    fn pause(&mut self, duration: Duration);
}

/// The set of ports passed to [`crate::RecordingCoordinator`].
pub struct CoordinatorPorts {
    pub clock: Box<dyn Clock + Send>,
    pub output: Box<dyn OutputFolder + Send>,
    pub pause: Box<dyn Pause + Send>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_iso_8601_with_the_offset_from_utc() {
        let time = |utc_offset_minutes| LocalDateTime {
            year: 2026,
            month: 1,
            day: 5,
            hour: 9,
            minute: 3,
            second: 7,
            utc_offset_minutes,
        };
        assert_eq!(time(540).iso8601(), "2026-01-05T09:03:07+09:00");
        assert_eq!(time(-330).iso8601(), "2026-01-05T09:03:07-05:30");
        assert_eq!(time(0).iso8601(), "2026-01-05T09:03:07+00:00");
    }
}
