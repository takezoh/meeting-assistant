//! Adding the microphone, recorded beside a browser tab recording, to that recording as its second audio track.
//!
//! The decisions (where the microphone goes on the recording's timeline, what the files are called, what is left when
//! something fails) are here; copying media and touching files go through ports.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Where the microphone recording goes on the timeline of the recording it is added to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// How much of the start of the microphone recording comes before the recording's time zero and is dropped.
    pub skip: Duration,
    /// How long after the recording's time zero the microphone starts, when it started later.
    pub delay: Duration,
}

/// Places a microphone recording whose time zero was `microphone_started` on a recording whose time zero was
/// `recording_started`. The microphone is meant to start first, so that it covers the whole recording.
pub fn placement(recording_started: Instant, microphone_started: Instant) -> Placement {
    match recording_started.checked_duration_since(microphone_started) {
        Some(skip) => Placement {
            skip,
            delay: Duration::ZERO,
        },
        None => Placement {
            skip: Duration::ZERO,
            delay: microphone_started.duration_since(recording_started),
        },
    }
}

/// Where the microphone is recorded beside `recording`, until it is added to it: `<name>.microphone.m4a`.
pub fn microphone_path(recording: &Path) -> PathBuf {
    with_suffix(recording, "microphone", "m4a")
}

/// Whether the microphone can be added to `recording`: only MP4 is copied track by track.
pub fn can_add_to(recording: &Path) -> bool {
    recording
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
}

/// Where the recording with the microphone added is written before it replaces the recording.
fn merging_path(recording: &Path) -> PathBuf {
    let extension = recording
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("mp4");
    with_suffix(recording, "merging", extension)
}

fn with_suffix(path: &Path, suffix: &str, extension: &str) -> PathBuf {
    let stem = path.file_stem().map(|stem| stem.to_string_lossy()).unwrap_or_default();
    path.with_file_name(format!("{stem}.{suffix}.{extension}"))
}

/// Records the microphone alone, beside a recording made elsewhere.
pub trait MicrophoneRecorder {
    /// Starts recording the microphone (`device`, or the default one) to `path`. Returns once it is being written, with
    /// the moment its timeline starts.
    fn start_microphone(&mut self, path: &Path, device: Option<String>) -> anyhow::Result<Instant>;
    /// Stops and finalizes the file. An error says why the recording ended early, if it did; what was recorded is kept.
    fn stop_microphone(&mut self) -> anyhow::Result<()>;
}

/// Adds a microphone recording to the recording it was made beside.
pub trait MicrophoneMerge {
    /// Adds `microphone` to `recording` as its last audio track, then removes `microphone`. On failure `recording` is
    /// as it was and `microphone` is kept.
    fn add_microphone(&self, recording: &Path, microphone: &Path, placement: Placement) -> anyhow::Result<()>;
    /// Removes a microphone recording that will not be added to anything.
    fn discard(&self, microphone: &Path);
}

/// Copies media between files without re-encoding it. Implemented by each OS.
pub trait Remuxer {
    /// Writes `output`: every track of `recording` unchanged, followed by the audio track of `microphone` placed by
    /// `placement` and cut at the end of `recording`.
    fn add_audio_track(
        &self,
        recording: &Path,
        microphone: &Path,
        placement: Placement,
        output: &Path,
    ) -> anyhow::Result<()>;
}

/// The file operations the merge needs.
pub trait RecordingFiles {
    /// Moves `from` to `to`, replacing `to`.
    fn replace(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
}

/// Adds the microphone to recordings by remuxing them, and discards microphone recordings that have nothing to be added
/// to.
pub struct Merger<X, F> {
    remuxer: X,
    files: F,
}

impl<X, F> Merger<X, F> {
    pub fn new(remuxer: X, files: F) -> Self {
        Self { remuxer, files }
    }
}

impl<X: Remuxer, F: RecordingFiles> MicrophoneMerge for Merger<X, F> {
    /// Writes the result beside the recording first, so that a failure leaves nothing else behind.
    fn add_microphone(&self, recording: &Path, microphone: &Path, placement: Placement) -> anyhow::Result<()> {
        let merging = merging_path(recording);
        let merged = self
            .remuxer
            .add_audio_track(recording, microphone, placement, &merging)
            .and_then(|()| Ok(self.files.replace(&merging, recording)?));
        if let Err(error) = merged {
            let _ = self.files.remove(&merging);
            return Err(error);
        }
        if let Err(error) = self.files.remove(microphone) {
            log::warn!(
                "The microphone was added to the recording, but its separate file could not be removed: {}: {error}",
                microphone.display()
            );
        }
        Ok(())
    }

    fn discard(&self, microphone: &Path) {
        if let Err(error) = self.files.remove(microphone) {
            log::warn!("Cannot remove {}: {error}", microphone.display());
        }
    }
}

#[cfg(test)]
pub mod fakes {
    //! A remuxer and files that record what was asked of them.

    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    pub struct FilesState {
        /// The files that exist.
        pub existing: BTreeSet<PathBuf>,
        /// (recording, microphone, placement, output) of each remux.
        pub remuxed: Vec<(PathBuf, PathBuf, Placement, PathBuf)>,
        pub remux_fails: bool,
        pub replace_fails: bool,
    }

    /// Fake files and remuxer sharing one state: remuxing creates the output, replacing moves it.
    #[derive(Clone, Default)]
    pub struct FakeMedia(pub Arc<Mutex<FilesState>>);

    impl FakeMedia {
        pub fn with_files(paths: &[&str]) -> Self {
            let media = Self::default();
            media.0.lock().unwrap().existing = paths.iter().map(PathBuf::from).collect();
            media
        }

        pub fn files(&self) -> Vec<String> {
            let state = self.0.lock().unwrap();
            state.existing.iter().map(|path| path.display().to_string()).collect()
        }
    }

    impl Remuxer for FakeMedia {
        fn add_audio_track(
            &self,
            recording: &Path,
            microphone: &Path,
            placement: Placement,
            output: &Path,
        ) -> anyhow::Result<()> {
            let mut state = self.0.lock().unwrap();
            state.remuxed.push((
                recording.to_owned(),
                microphone.to_owned(),
                placement,
                output.to_owned(),
            ));
            // A failed remux may leave a partial file behind.
            state.existing.insert(output.to_owned());
            anyhow::ensure!(!state.remux_fails, "cannot read the recording");
            Ok(())
        }
    }

    impl RecordingFiles for FakeMedia {
        fn replace(&self, from: &Path, to: &Path) -> io::Result<()> {
            let mut state = self.0.lock().unwrap();
            if state.replace_fails || !state.existing.remove(from) {
                return Err(io::Error::other("cannot replace"));
            }
            state.existing.insert(to.to_owned());
            Ok(())
        }

        fn remove(&self, path: &Path) -> io::Result<()> {
            if self.0.lock().unwrap().existing.remove(path) {
                Ok(())
            } else {
                Err(io::ErrorKind::NotFound.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::FakeMedia;
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn a_microphone_that_started_first_is_cut_to_the_recording_start() {
        let microphone = Instant::now();
        let recording = microphone + SECOND / 4;
        assert_eq!(
            placement(recording, microphone),
            Placement {
                skip: SECOND / 4,
                delay: Duration::ZERO
            }
        );
    }

    #[test]
    fn a_microphone_that_started_later_is_delayed() {
        let recording = Instant::now();
        let microphone = recording + SECOND / 10;
        assert_eq!(
            placement(recording, microphone),
            Placement {
                skip: Duration::ZERO,
                delay: SECOND / 10
            }
        );
        assert_eq!(
            placement(recording, recording),
            Placement {
                skip: Duration::ZERO,
                delay: Duration::ZERO
            }
        );
    }

    #[test]
    fn the_microphone_is_recorded_beside_the_recording() {
        assert_eq!(
            microphone_path(Path::new("/r/2026-11-19_09-30-23_GoogleMeet.mp4")),
            Path::new("/r/2026-11-19_09-30-23_GoogleMeet.microphone.m4a")
        );
    }

    #[test]
    fn only_mp4_recordings_take_the_microphone() {
        assert!(can_add_to(Path::new("a.mp4")));
        assert!(can_add_to(Path::new("a.MP4")));
        assert!(!can_add_to(Path::new("a.webm")));
        assert!(!can_add_to(Path::new("a")));
    }

    const RECORDING: &str = "/r/meeting.mp4";
    const MICROPHONE: &str = "/r/meeting.microphone.m4a";
    const PLACEMENT: Placement = Placement {
        skip: Duration::from_millis(30),
        delay: Duration::ZERO,
    };

    fn merger(media: &FakeMedia) -> Merger<FakeMedia, FakeMedia> {
        Merger::new(media.clone(), media.clone())
    }

    #[test]
    fn the_recording_is_replaced_by_one_with_the_microphone_and_the_microphone_file_is_removed() {
        let media = FakeMedia::with_files(&[RECORDING, MICROPHONE]);

        merger(&media)
            .add_microphone(Path::new(RECORDING), Path::new(MICROPHONE), PLACEMENT)
            .unwrap();

        assert_eq!(
            media.0.lock().unwrap().remuxed,
            [(
                RECORDING.into(),
                MICROPHONE.into(),
                PLACEMENT,
                "/r/meeting.merging.mp4".into()
            )]
        );
        assert_eq!(media.files(), [RECORDING]);
    }

    #[test]
    fn a_failed_remux_leaves_the_recording_and_the_microphone_and_nothing_else() {
        let media = FakeMedia::with_files(&[RECORDING, MICROPHONE]);
        media.0.lock().unwrap().remux_fails = true;

        let error = merger(&media)
            .add_microphone(Path::new(RECORDING), Path::new(MICROPHONE), PLACEMENT)
            .unwrap_err();

        assert!(error.to_string().contains("cannot read"), "{error}");
        assert_eq!(media.files(), [MICROPHONE, RECORDING]);
    }

    #[test]
    fn a_failed_replace_leaves_the_recording_and_the_microphone_and_nothing_else() {
        let media = FakeMedia::with_files(&[RECORDING, MICROPHONE]);
        media.0.lock().unwrap().replace_fails = true;

        assert!(
            merger(&media)
                .add_microphone(Path::new(RECORDING), Path::new(MICROPHONE), PLACEMENT)
                .is_err()
        );
        assert_eq!(media.files(), [MICROPHONE, RECORDING]);
    }

    #[test]
    fn a_discarded_microphone_recording_is_removed() {
        let media = FakeMedia::with_files(&[MICROPHONE]);
        merger(&media).discard(Path::new(MICROPHONE));
        assert!(media.files().is_empty());
    }
}
