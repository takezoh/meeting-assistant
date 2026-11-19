//! The real implementations of core's ports and of the app's file ports. Only this module touches the clock, the file
//! system, and thread sleeping. None of them depend on the OS (they use only the standard library and chrono).

pub mod config_file;

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use chrono::{Datelike, Timelike};
use meeting_assistant_core::{Clock, CoordinatorPorts, LocalDateTime, OutputFolder, Pause};

use crate::recording::merge::{Placement, RecordingFiles, Remuxer};
use crate::recording::mp4::{self, ReadAt};

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

/// A file read at any position.
pub struct DiskFile(Mutex<File>);

impl DiskFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self(Mutex::new(File::open(path)?)))
    }
}

impl ReadAt for DiskFile {
    fn len(&self) -> io::Result<u64> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .metadata()?
            .len())
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let mut file = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)
    }
}

/// Adds a track by rewriting the MP4's structure (see [`mp4`]), the same on every OS.
pub struct Mp4Remuxer;

impl Remuxer for Mp4Remuxer {
    fn add_audio_track(
        &self,
        recording: &Path,
        microphone: &Path,
        placement: Placement,
        output: &Path,
    ) -> anyhow::Result<()> {
        let recording = DiskFile::open(recording).context("cannot open the recording")?;
        let microphone = DiskFile::open(microphone).context("cannot open the microphone recording")?;
        let pieces = mp4::add_audio_track(&recording, &microphone, placement)?;
        let mut out =
            BufWriter::new(File::create(output).with_context(|| format!("cannot create {}", output.display()))?);
        mp4::write_pieces(&pieces, &recording, &microphone, &mut out)?;
        out.into_inner().map_err(|error| error.into_error())?.sync_all()?;
        Ok(())
    }
}

/// The file system, for adding the microphone to a recording.
pub struct DiskFiles;

impl RecordingFiles for DiskFiles {
    fn replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        // Replaces an existing `to` on every supported OS.
        std::fs::rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
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

    /// Adds the microphone file named by `MEETING_ASSISTANT_MERGE_MICROPHONE` to the recording named by
    /// `MEETING_ASSISTANT_MERGE_RECORDING`, writing `MEETING_ASSISTANT_MERGE_OUTPUT`: for checking the merge with a
    /// browser's real recording by hand.
    #[test]
    #[ignore = "needs files named in the environment"]
    fn merges_the_files_named_in_the_environment() {
        let path = |name: &str| PathBuf::from(std::env::var(name).unwrap());
        Mp4Remuxer
            .add_audio_track(
                &path("MEETING_ASSISTANT_MERGE_RECORDING"),
                &path("MEETING_ASSISTANT_MERGE_MICROPHONE"),
                Placement {
                    skip: Duration::from_millis(250),
                    delay: Duration::ZERO,
                },
                &path("MEETING_ASSISTANT_MERGE_OUTPUT"),
            )
            .unwrap();
    }

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
