//! Writing what is known of a recording into its file once it is finished. Every recording goes through this, whether
//! the OS or the browser recorded it.
//!
//! The decisions (which keys the file gets, which files can take them, what is left when writing fails) are here;
//! copying the file goes through ports.

use std::path::Path;

use meeting_assistant_core::{RecordingMetadata, handover};

use super::merge::{RecordingFiles, Remuxer, with_suffix};

/// The prefix of every key the app writes into a file, in the reverse-DNS form QuickTime metadata keys take.
pub const KEY_PREFIX: &str = "com.meeting-assistant.";

/// The keys and values written into a recording's file: the app as `com.meeting-assistant.app`, when the recording
/// started as `com.meeting-assistant.started_at` (ISO 8601 with the offset from UTC), then each entry its meeting's app
/// told, in order, as `com.meeting-assistant.meeting.<key>`.
pub fn file_entries(metadata: &RecordingMetadata) -> Vec<(String, String)> {
    std::iter::once((format!("{KEY_PREFIX}app"), metadata.app.clone()))
        .chain(
            metadata
                .started_at
                .map(|started| (format!("{KEY_PREFIX}started_at"), started.iso8601())),
        )
        .chain(
            metadata
                .meeting
                .entries()
                .iter()
                .map(|(key, value)| (format!("{KEY_PREFIX}meeting.{key}"), value.clone())),
        )
        .collect()
}

/// Whether `recording` can take metadata: only MP4 is rewritten.
pub fn can_describe(recording: &Path) -> bool {
    recording
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
}

/// Writes what is known of a recording into its finished file.
pub trait Annotate {
    /// Writes `metadata` into `recording`. On failure `recording` is as it was.
    fn annotate(&self, recording: &Path, metadata: &RecordingMetadata) -> anyhow::Result<()>;
}

/// Writes metadata by rewriting the recording beside it and putting the result in its place.
pub struct Annotator<X, F> {
    remuxer: X,
    files: F,
}

impl<X, F> Annotator<X, F> {
    pub fn new(remuxer: X, files: F) -> Self {
        Self { remuxer, files }
    }
}

impl<X: Remuxer, F: RecordingFiles> Annotate for Annotator<X, F> {
    fn annotate(&self, recording: &Path, metadata: &RecordingMetadata) -> anyhow::Result<()> {
        anyhow::ensure!(can_describe(recording), "only an MP4 file takes metadata");
        let annotating = with_suffix(recording, handover::ANNOTATING, "mp4");
        let written = self
            .remuxer
            .add_metadata(recording, &file_entries(metadata), &annotating)
            .and_then(|()| Ok(self.files.replace(&annotating, recording)?));
        if let Err(error) = written {
            let _ = self.files.remove(&annotating);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use meeting_assistant_core::{LocalDateTime, MeetingMetadata};

    use super::super::merge::fakes::FakeMedia;
    use super::*;

    fn slack() -> RecordingMetadata {
        RecordingMetadata {
            app: "Slack".into(),
            started_at: Some(LocalDateTime {
                year: 2026,
                month: 11,
                day: 19,
                hour: 19,
                minute: 1,
                second: 56,
                utc_offset_minutes: 540,
            }),
            meeting: MeetingMetadata::new([("conversation", "standup"), ("workspace", "team")]),
        }
    }

    #[test]
    fn the_app_and_the_start_come_first_and_each_entry_of_the_meeting_follows_under_its_own_key() {
        let entries = file_entries(&slack());
        let pairs: Vec<(&str, &str)> = entries.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("com.meeting-assistant.app", "Slack"),
                ("com.meeting-assistant.started_at", "2026-11-19T19:01:56+09:00"),
                ("com.meeting-assistant.meeting.conversation", "standup"),
                ("com.meeting-assistant.meeting.workspace", "team"),
            ]
        );
    }

    #[test]
    fn writes_beside_the_recording_and_puts_the_result_in_its_place() {
        let media = FakeMedia::with_files(&["/r/a.mp4"]);
        Annotator::new(media.clone(), media.clone())
            .annotate(Path::new("/r/a.mp4"), &slack())
            .unwrap();
        let state = media.0.lock().unwrap();
        assert_eq!(state.described[0].0, PathBuf::from("/r/a.mp4"));
        assert_eq!(state.described[0].2, PathBuf::from("/r/a.annotating.mp4"));
        assert_eq!(state.existing.iter().collect::<Vec<_>>(), [&PathBuf::from("/r/a.mp4")]);
    }

    #[test]
    fn a_failure_leaves_the_recording_as_it_was_and_nothing_beside_it() {
        let media = FakeMedia::with_files(&["/r/a.mp4"]);
        media.0.lock().unwrap().remux_fails = true;
        let result = Annotator::new(media.clone(), media.clone()).annotate(Path::new("/r/a.mp4"), &slack());
        assert!(result.is_err());
        assert_eq!(media.files(), ["/r/a.mp4"]);
    }

    #[test]
    fn a_file_other_than_mp4_is_left_alone() {
        let media = FakeMedia::with_files(&["/r/a.webm"]);
        let result = Annotator::new(media.clone(), media.clone()).annotate(Path::new("/r/a.webm"), &slack());
        assert!(result.is_err());
        assert!(media.0.lock().unwrap().described.is_empty());
    }
}
