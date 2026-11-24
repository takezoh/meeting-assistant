use std::path::PathBuf;

use crate::metadata::RecordingMetadata;
use crate::signals::BrowserTab;

/// What to record as video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoTarget {
    /// One window (the `WindowEntry::id` value).
    Window(u64),
    /// One whole display. `None` is the main display.
    Display(Option<String>),
    /// No video target yet. Starts with black video and waits for a window to be given via [`Recorder::retarget`].
    Blank,
    /// One browser tab. The browser extension records both video and audio (the tab's sound and the microphone).
    BrowserTab(BrowserTab),
}

/// A recording instruction.
///
/// The other party's audio is only the sound played by the app if `audio_process_id` is set, otherwise all sound
/// going out of the device `render_device_id` (or the default device if that is not set either).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingRequest {
    pub output_path: PathBuf,
    pub video: VideoTarget,
    pub audio_process_id: Option<u32>,
    pub render_device_id: Option<String>,
    /// The microphone. If `None`, the default device is used.
    pub capture_device_id: Option<String>,
    /// Whether the microphone is recorded. When not, its track is silent and the microphone is not opened. Only a
    /// recording of the desktop or a window started by hand leaves it out.
    pub microphone: bool,
    /// What to write into the file besides its media, as known at start; see [`Recorder::describe`].
    pub metadata: RecordingMetadata,
}

/// A sound a recording is asked to record on a track of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// What the meeting plays: the other participants.
    OtherSide,
    /// The user's microphone.
    Microphone,
}

/// A sound a started recording was asked for but does not have, because its source could not be opened. Its track
/// stays silent; nothing else is recorded in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSound {
    pub sound: Sound,
    /// Why its source could not be opened.
    pub reason: String,
}

/// A recording that has started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// The file it writes to. That is the request's `output_path`, unless the recorder gave the file the extension of
    /// a different container than the one asked for, as a tab recorded as WebM does.
    pub path: PathBuf,
    /// The sounds asked for that it does not record.
    pub missing: Vec<MissingSound>,
}

/// A port that records the display and audio into one file.
pub trait Recorder {
    /// Returns once recording has actually started. Errors if it cannot start. A start that did not finish in time
    /// may still be preparing, and creating and writing the file, after it returned its error.
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<Started>;

    /// Returns once the file is finalized. Does nothing if not recording.
    fn stop(&mut self) -> anyhow::Result<()>;

    /// Switches the video target of the recording to the given window. The file is not split.
    /// Used to give the meeting's window to a recording started with no video target, or a window that appeared
    /// later to one whose window closed.
    fn retarget(&mut self, window: u64) -> anyhow::Result<()>;

    /// Replaces what is written into the file besides its media, when more is learned of the meeting during the
    /// recording. It is written as the file is finished. Does nothing if not recording.
    fn describe(&mut self, metadata: &RecordingMetadata);

    /// If a successfully started recording failed without a stop instruction, returns the cause once.
    /// By the time it is returned, the recorder is back to the not-recording state.
    fn take_failure(&mut self) -> Option<String>;
}
