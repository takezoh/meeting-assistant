use std::path::PathBuf;

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
}

/// A port that records the display and audio into one file.
pub trait Recorder {
    /// Returns once recording has actually started, with the file it writes to. That is `request.output_path`,
    /// unless the recorder gave the file the extension of a different container than the one asked for, as a tab
    /// recorded as WebM does. Errors if it cannot start.
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<PathBuf>;

    /// Returns once the file is finalized. Does nothing if not recording.
    fn stop(&mut self) -> anyhow::Result<()>;

    /// Switches the video target of the recording to the given window. The file is not split.
    /// Used when the target window is closed and recording moves to another window of the same app,
    /// or to give a window to a recording started with no video target.
    fn retarget(&mut self, window: u64) -> anyhow::Result<()>;

    /// If a successfully started recording failed without a stop instruction, returns the cause once.
    /// By the time it is returned, the recorder is back to the not-recording state.
    fn take_failure(&mut self) -> Option<String>;
}
