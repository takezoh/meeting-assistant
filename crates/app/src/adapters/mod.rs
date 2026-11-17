//! The real implementations of core's ports. Only this module touches the clock, the file system, and thread sleeping.
//! None of them depend on the OS (they use only the standard library and chrono).

pub mod config_file;

use std::path::PathBuf;
use std::time::Duration;

use chrono::{Datelike, Timelike};
use meeting_assistant_core::{Clock, CoordinatorPorts, LocalDateTime, OutputFolder, Pause};

/// The OS clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> LocalDateTime {
        let now = chrono::Local::now();
        LocalDateTime {
            year: now.year(),
            month: now.month(),
            day: now.day(),
            hour: now.hour(),
            minute: now.minute(),
            second: now.second(),
        }
    }
}

/// A folder on disk.
pub struct DiskFolder(pub PathBuf);

impl OutputFolder for DiskFolder {
    fn prepare(&mut self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.0)?;
        Ok(())
    }

    fn contains(&self, file_name: &str) -> bool {
        self.0.join(file_name).exists()
    }

    fn path_of(&self, file_name: &str) -> PathBuf {
        self.0.join(file_name)
    }
}

/// Blocks the current thread and waits.
pub struct ThreadPause;

impl Pause for ThreadPause {
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A set of ports made up only of real implementations.
pub fn real_ports(output_directory: PathBuf) -> CoordinatorPorts {
    CoordinatorPorts {
        clock: Box::new(SystemClock),
        output: Box::new(DiskFolder(output_directory)),
        pause: Box::new(ThreadPause),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_missing_folder_including_parents() {
        let root = tempfile::tempdir().unwrap();
        let mut folder = DiskFolder(root.path().join("a").join("b"));

        folder.prepare().unwrap();

        assert!(root.path().join("a").join("b").is_dir());
        folder.prepare().unwrap();
    }

    #[test]
    fn reports_only_existing_files_as_existing() {
        let root = tempfile::tempdir().unwrap();
        let folder = DiskFolder(root.path().to_owned());
        std::fs::write(root.path().join("existing.mp4"), []).unwrap();

        assert!(folder.contains("existing.mp4"));
        assert!(!folder.contains("missing.mp4"));
        assert_eq!(folder.path_of("missing.mp4"), root.path().join("missing.mp4"));
    }

    #[test]
    fn clock_returns_a_plausible_datetime() {
        let now = SystemClock.now();

        assert!(now.year >= 2026);
        assert!((1..=12).contains(&now.month) && (1..=31).contains(&now.day));
        assert!(now.hour < 24 && now.minute < 60 && now.second < 61);
    }
}
