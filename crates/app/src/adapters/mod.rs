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
use meeting_assistant_core::{Clock, CoordinatorPorts, LocalDateTime, OutputFolder, Pause, naming};

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
            utc_offset_minutes: now.offset().local_minus_utc() / 60,
        }
    }
}

/// The recordings folder on disk, with its in-progress folder inside it. Being on the same volume, a finished file
/// moves into the recordings folder by a rename, all at once.
pub struct DiskFolder(pub PathBuf);

impl DiskFolder {
    fn in_progress_folder(&self) -> PathBuf {
        self.0.join(naming::IN_PROGRESS_FOLDER)
    }
}

impl OutputFolder for DiskFolder {
    fn prepare(&mut self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.in_progress_folder())?;
        Ok(())
    }

    fn contains(&self, file_name: &str) -> bool {
        self.0.join(file_name).exists()
    }

    fn in_progress(&self) -> anyhow::Result<Vec<String>> {
        let entries = match std::fs::read_dir(self.in_progress_folder()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).context("cannot read the in-progress folder"),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && let Some(name) = entry.file_name().to_str()
            {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    fn in_progress_path(&self, file_name: &str) -> PathBuf {
        self.in_progress_folder().join(file_name)
    }

    fn publish(&mut self, file_name: &str, target_name: &str) -> anyhow::Result<PathBuf> {
        let target = self.0.join(target_name);
        // A rename would replace an existing file on macOS; the name was chosen free, so this only guards a race.
        anyhow::ensure!(!target.exists(), "{} already exists", target.display());
        std::fs::rename(self.in_progress_path(file_name), &target)
            .with_context(|| format!("cannot move {file_name} to {}", target.display()))?;
        // The in-progress folder exists only while something is in it; it is made again for the next recording.
        let _ = std::fs::remove_dir(self.in_progress_folder());
        Ok(target)
    }

    fn discard(&mut self, file_name: &str) -> anyhow::Result<()> {
        std::fs::remove_file(self.in_progress_path(file_name)).with_context(|| format!("cannot remove {file_name}"))
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

/// Adds a track or metadata by rewriting the MP4's structure (see [`mp4`]), the same on every OS.
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

    fn add_metadata(&self, recording: &Path, entries: &[(String, String)], output: &Path) -> anyhow::Result<()> {
        let recording = DiskFile::open(recording).context("cannot open the recording")?;
        let pieces = mp4::with_metadata(&recording, entries)?;
        let mut out =
            BufWriter::new(File::create(output).with_context(|| format!("cannot create {}", output.display()))?);
        mp4::write_pieces(&pieces, &recording, &Vec::new(), &mut out)?;
        out.into_inner().map_err(|error| error.into_error())?.sync_all()?;
        Ok(())
    }
}

/// The file system, for adding the microphone or metadata to a recording.
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

    fn folder() -> (tempfile::TempDir, DiskFolder) {
        let root = tempfile::tempdir().unwrap();
        let folder = DiskFolder(root.path().join("MeetingAssistant"));
        (root, folder)
    }

    #[test]
    fn a_recording_is_written_in_progress_and_moved_into_the_recordings_folder() {
        let (_root, mut folder) = folder();
        assert!(
            folder.in_progress().unwrap().is_empty(),
            "nothing in progress before the folder exists"
        );
        folder.prepare().unwrap();
        let writing = folder.in_progress_path("a.mp4");
        assert_eq!(writing, folder.0.join("in-progress").join("a.mp4"));
        std::fs::write(&writing, b"video").unwrap();
        assert_eq!(folder.in_progress().unwrap(), ["a.mp4"]);
        assert!(
            !folder.contains("a.mp4"),
            "a file in progress is not in the recordings folder"
        );

        let moved = folder.publish("a.mp4", "a.mp4").unwrap();

        assert_eq!(moved, folder.0.join("a.mp4"));
        assert_eq!(std::fs::read(&moved).unwrap(), b"video");
        assert!(folder.contains("a.mp4"));
        assert!(
            !folder.0.join("in-progress").exists(),
            "the empty in-progress folder is removed"
        );
    }

    #[test]
    fn the_in_progress_folder_stays_while_something_is_still_in_it() {
        let (_root, mut folder) = folder();
        folder.prepare().unwrap();
        std::fs::write(folder.in_progress_path("a.mp4"), b"a").unwrap();
        std::fs::write(folder.in_progress_path("b.mp4"), b"b").unwrap();

        folder.publish("a.mp4", "a.mp4").unwrap();

        assert_eq!(folder.in_progress().unwrap(), ["b.mp4"]);
    }

    #[test]
    fn moving_never_replaces_a_file_in_the_recordings_folder() {
        let (_root, mut folder) = folder();
        folder.prepare().unwrap();
        std::fs::write(folder.0.join("a.mp4"), b"kept").unwrap();
        std::fs::write(folder.in_progress_path("a.mp4"), b"new").unwrap();

        assert!(folder.publish("a.mp4", "a.mp4").is_err());

        assert_eq!(std::fs::read(folder.0.join("a.mp4")).unwrap(), b"kept");
        assert_eq!(std::fs::read(folder.in_progress_path("a.mp4")).unwrap(), b"new");
    }

    #[test]
    fn an_unfinished_copy_is_removed_from_the_in_progress_folder() {
        let (_root, mut folder) = folder();
        folder.prepare().unwrap();
        std::fs::write(folder.in_progress_path("a.annotating.mp4"), b"part").unwrap();

        folder.discard("a.annotating.mp4").unwrap();

        assert!(folder.in_progress().unwrap().is_empty());
    }

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

    /// Writes metadata into `MEETING_ASSISTANT_DESCRIBE_RECORDING`, writing `MEETING_ASSISTANT_DESCRIBE_OUTPUT`: for
    /// checking with real recordings and other readers by hand.
    #[test]
    #[ignore = "needs files named in the environment"]
    fn writes_metadata_into_the_file_named_in_the_environment() {
        let path = |name: &str| PathBuf::from(std::env::var(name).unwrap());
        let entries = [
            ("com.meeting-assistant.app", "Slack"),
            ("com.meeting-assistant.started_at", "2026-11-19T18:35:35+09:00"),
            ("com.meeting-assistant.meeting.conversation", "dev-gcp-errors"),
            ("com.meeting-assistant.meeting.workspace", "tkz"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        Mp4Remuxer
            .add_metadata(
                &path("MEETING_ASSISTANT_DESCRIBE_RECORDING"),
                &entries,
                &path("MEETING_ASSISTANT_DESCRIBE_OUTPUT"),
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
        assert_eq!(
            folder.in_progress_path("missing.mp4"),
            root.path().join("in-progress").join("missing.mp4")
        );
    }

    #[test]
    fn clock_returns_a_plausible_datetime() {
        let now = SystemClock.now();

        assert!(now.year >= 2026);
        assert!((1..=12).contains(&now.month) && (1..=31).contains(&now.day));
        assert!(now.hour < 24 && now.minute < 60 && now.second < 61);
    }
}
