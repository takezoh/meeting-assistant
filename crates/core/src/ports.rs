//! Small ports that the recording coordinator relies on for the outside world.
//!
//! The ports for detection signals ([`crate::SignalSource`]) and recording ([`crate::Recorder`]) sit next to the
//! data they handle.
//! This module holds the rest: the clock, the output folder, and waiting.

use std::path::PathBuf;
use std::time::Duration;

/// The user's local date and time. Used in recording file names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
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
