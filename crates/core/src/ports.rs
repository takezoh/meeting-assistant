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

/// The folder where recordings are saved, and the folder inside it where they are written while in progress.
///
/// A recording is written in the in-progress folder and moved into the recordings folder once nothing writes it any
/// more, so a file in the recordings folder is never still being written.
pub trait OutputFolder {
    /// Makes both folders (creating them if missing) ready for writing.
    fn prepare(&mut self) -> anyhow::Result<()>;

    /// Whether the recordings folder has a file with that name.
    fn contains(&self, file_name: &str) -> bool;

    /// The names of the files in the in-progress folder, in any order. Empty when the folder does not exist.
    fn in_progress(&self) -> anyhow::Result<Vec<String>>;

    /// Where a file of that name is written while in progress.
    fn in_progress_path(&self, file_name: &str) -> PathBuf;

    /// Moves `file_name` from the in-progress folder into the recordings folder as `target_name`, which must not exist
    /// there. Returns its new location.
    fn publish(&mut self, file_name: &str, target_name: &str) -> anyhow::Result<PathBuf>;

    /// Removes `file_name` from the in-progress folder.
    fn discard(&mut self, file_name: &str) -> anyhow::Result<()>;
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
