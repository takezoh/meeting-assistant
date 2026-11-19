//! Decides what to capture and at what size to write, from the recording request and settings. No side effects.

use meeting_assistant_core::{AudioConfig, RecordingRequest, VideoTarget};

/// All audio inputs are normalized to this format (16-bit) before being handled.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// The video target to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoSource {
    /// A single window (the value of `WindowEntry::id`).
    Window(u64),
    /// A single display. `None`, or a display that cannot be found, means the main display.
    Display(Option<String>),
}

impl VideoSource {
    pub fn is_window(&self) -> bool {
        matches!(self, Self::Window(_))
    }
}

/// The audio inputs to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSource {
    /// Sound produced by one app (the given process and its descendants).
    ProcessAudio(u32),
    /// All sound coming out of an output device. `None` means the default device.
    DeviceAudio(Option<String>),
    /// The microphone. `None` means the default device.
    Microphone(Option<String>),
}

impl AudioSource {
    /// Name shown in logs.
    pub fn label(&self) -> String {
        match self {
            Self::ProcessAudio(process_id) => format!("app audio (pid {process_id})"),
            Self::DeviceAudio(_) => "output device audio".to_owned(),
            Self::Microphone(_) => "microphone".to_owned(),
        }
    }
}

/// The audio tracks of a recording, in the order they are written to the file. Analysis relies on the order: the other
/// side first, the user's microphone second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    /// What the meeting plays: the meeting app's audio, or all output audio.
    OtherSide,
    /// The user's microphone.
    Microphone,
}

impl Track {
    /// Name shown in logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::OtherSide => "other side",
            Self::Microphone => "microphone",
        }
    }
}

/// The video of a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoPlan {
    /// No video track: a recording of sound alone.
    None,
    /// A video track that starts black, for a recording started without a target.
    Blank,
    /// A video track of this target.
    Capture(VideoSource),
}

/// What one recording captures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturePlan {
    pub video: VideoPlan,
    /// Every audio track of the file, in order, with the source that fills it. A track without a source, or whose
    /// source cannot be captured, holds silence: the layout of the file does not depend on what could be captured.
    pub tracks: Vec<(Track, Option<AudioSource>)>,
}

/// Format of the file to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFormat {
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    /// Quality (1 to 100).
    pub quality: u32,
    /// Average bitrate (bps) for encoders that do not support a quality setting.
    pub fallback_bitrate: u32,
    /// Whether the file has a video track. Without one, the size and video settings are not used.
    pub has_video: bool,
    /// The audio tracks, in the order they are written.
    pub audio_tracks: Vec<Track>,
}

/// The plan for recording a meeting or the screen: video of the target, and two audio tracks, the other side and the
/// microphone.
///
/// For the other side, if the target app is known, only that app's audio is used. If the app's audio cannot be
/// captured, we do not substitute all sound from the output device (so that a user who meant to record only the app
/// is not silently recorded with other sounds). An input turned off in the settings leaves its track silent.
pub fn meeting_plan(request: &RecordingRequest, config: &AudioConfig) -> anyhow::Result<CapturePlan> {
    let video = match &request.video {
        VideoTarget::Window(window) => VideoPlan::Capture(VideoSource::Window(*window)),
        VideoTarget::Display(display) => VideoPlan::Capture(VideoSource::Display(display.clone())),
        VideoTarget::Blank => VideoPlan::Blank,
        VideoTarget::BrowserTab(_) => anyhow::bail!("a browser tab can only be recorded through the browser extension"),
    };
    let other_side = config.record_system_audio.then(|| match request.audio_process_id {
        Some(process_id) => AudioSource::ProcessAudio(process_id),
        None => AudioSource::DeviceAudio(request.render_device_id.clone()),
    });
    let microphone = config
        .record_microphone
        .then(|| AudioSource::Microphone(request.capture_device_id.clone()));
    Ok(CapturePlan {
        video,
        tracks: vec![(Track::OtherSide, other_side), (Track::Microphone, microphone)],
    })
}

/// The plan for recording the microphone alone, beside a recording made elsewhere (a browser tab). `None` means the
/// default device. With the microphone turned off in the settings, the track is silent.
pub fn microphone_plan(config: &AudioConfig, device: Option<String>) -> CapturePlan {
    CapturePlan {
        video: VideoPlan::None,
        tracks: vec![(
            Track::Microphone,
            config.record_microphone.then_some(AudioSource::Microphone(device)),
        )],
    }
}

#[cfg(test)]
mod tests {
    use meeting_assistant_core::BrowserTab;

    use super::*;

    fn request(audio_process_id: Option<u32>) -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video: VideoTarget::Blank,
            audio_process_id,
            render_device_id: Some("speaker".into()),
            capture_device_id: Some("mic".into()),
        }
    }

    fn video(target: VideoTarget) -> VideoPlan {
        let request = RecordingRequest {
            video: target,
            ..request(None)
        };
        meeting_plan(&request, &AudioConfig::default()).unwrap().video
    }

    fn tracks(audio_process_id: Option<u32>, config: &AudioConfig) -> Vec<(Track, Option<AudioSource>)> {
        meeting_plan(&request(audio_process_id), config).unwrap().tracks
    }

    #[test]
    fn video_target_follows_the_request() {
        assert_eq!(
            video(VideoTarget::Window(7)),
            VideoPlan::Capture(VideoSource::Window(7))
        );
        assert_eq!(
            video(VideoTarget::Display(Some("DISPLAY2".into()))),
            VideoPlan::Capture(VideoSource::Display(Some("DISPLAY2".into())))
        );
        assert_eq!(
            video(VideoTarget::Display(None)),
            VideoPlan::Capture(VideoSource::Display(None))
        );
        assert_eq!(video(VideoTarget::Blank), VideoPlan::Blank);
    }

    #[test]
    fn browser_tab_cannot_be_captured() {
        let tab = RecordingRequest {
            video: VideoTarget::BrowserTab(BrowserTab { link: 1, tab: 2 }),
            ..request(None)
        };
        assert!(meeting_plan(&tab, &AudioConfig::default()).is_err());
    }

    #[test]
    fn the_other_side_is_the_app_audio_when_the_target_app_is_known_and_the_microphone_comes_second() {
        assert_eq!(
            tracks(Some(100), &AudioConfig::default()),
            [
                (Track::OtherSide, Some(AudioSource::ProcessAudio(100))),
                (Track::Microphone, Some(AudioSource::Microphone(Some("mic".into()))))
            ]
        );
    }

    #[test]
    fn the_other_side_is_the_output_device_audio_when_the_target_app_is_unknown() {
        assert_eq!(
            tracks(None, &AudioConfig::default())[0],
            (Track::OtherSide, Some(AudioSource::DeviceAudio(Some("speaker".into()))))
        );
    }

    #[test]
    fn an_input_disabled_in_settings_keeps_its_track_without_a_source() {
        let only_system = AudioConfig {
            record_system_audio: true,
            record_microphone: false,
        };
        let nothing = AudioConfig {
            record_system_audio: false,
            record_microphone: false,
        };
        assert_eq!(
            tracks(Some(100), &only_system),
            [
                (Track::OtherSide, Some(AudioSource::ProcessAudio(100))),
                (Track::Microphone, None)
            ]
        );
        assert_eq!(
            tracks(Some(100), &nothing),
            [(Track::OtherSide, None), (Track::Microphone, None)],
            "the layout of the file stays the same"
        );
    }

    #[test]
    fn the_microphone_alone_has_no_video_and_one_track() {
        assert_eq!(
            microphone_plan(&AudioConfig::default(), Some("mic".into())),
            CapturePlan {
                video: VideoPlan::None,
                tracks: vec![(Track::Microphone, Some(AudioSource::Microphone(Some("mic".into()))))],
            }
        );
        let off = AudioConfig {
            record_system_audio: true,
            record_microphone: false,
        };
        assert_eq!(microphone_plan(&off, None).tracks, [(Track::Microphone, None)]);
    }
}
