//! Decides what to capture and at what size to write, from the recording request and settings. No side effects.

use meeting_assistant_core::{AudioConfig, RecordingRequest, VideoTarget};

/// All audio inputs are normalized to this format (16-bit) before being handled.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Minimum video size, so the picture never goes below what the encoder can handle.
const MIN_SIZE: (u32, u32) = (320, 240);
/// Largest video size: 1080p, laid the way the target is (1080 by 1920 for a tall window). A meeting needs no more,
/// and every hardware H.264 encoder takes it.
const MAX_SIZE: (u32, u32) = (1920, 1080);
/// Size of a recording started without a video target. If a window is specified later, it is fitted into this size.
const BLANK_SIZE: (u32, u32) = (1280, 720);

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
    pub has_audio: bool,
}

/// What to capture as video. `None` when starting without a target.
pub fn video_source(target: &VideoTarget) -> anyhow::Result<Option<VideoSource>> {
    match target {
        VideoTarget::Window(window) => Ok(Some(VideoSource::Window(*window))),
        VideoTarget::Display(display) => Ok(Some(VideoSource::Display(display.clone()))),
        VideoTarget::Blank => Ok(None),
        VideoTarget::BrowserTab(_) => anyhow::bail!("a browser tab can only be recorded through the browser extension"),
    }
}

/// What to capture as audio.
///
/// For the other party's audio, if the target app is known, only that app's audio is used.
/// If the app's audio cannot be captured, we do not substitute all sound from the output device
/// (so that a user who meant to record only the app is not silently recorded with other sounds).
pub fn audio_sources(request: &RecordingRequest, config: &AudioConfig) -> Vec<AudioSource> {
    let mut sources = Vec::new();
    if config.record_system_audio {
        sources.push(match request.audio_process_id {
            Some(process_id) => AudioSource::ProcessAudio(process_id),
            None => AudioSource::DeviceAudio(request.render_device_id.clone()),
        });
    }
    if config.record_microphone {
        sources.push(AudioSource::Microphone(request.capture_device_id.clone()));
    }
    sources
}

/// Size of the video to write. `source` is the size of the capture target, `None` when starting without a target.
///
/// A target larger than 1080p is scaled down to fit it with its aspect ratio kept; a smaller one is not scaled up.
/// H.264 requires even dimensions, so odd sizes are trimmed by one pixel.
/// From then on, video is written at this size (it cannot change midway).
pub fn canvas_size(source: Option<(u32, u32)>) -> (u32, u32) {
    let (width, height) = source.unwrap_or(BLANK_SIZE);
    let (max_width, max_height) = if width >= height {
        MAX_SIZE
    } else {
        (MAX_SIZE.1, MAX_SIZE.0)
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width.max(1)),
            f64::from(max_height) / f64::from(height.max(1)),
        ),
    );
    let fitted = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let (width, height) = (fitted(width), fitted(height));
    ((width & !1).max(MIN_SIZE.0), (height & !1).max(MIN_SIZE.1))
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

    #[test]
    fn video_target_follows_the_request() {
        assert_eq!(
            video_source(&VideoTarget::Window(7)).unwrap(),
            Some(VideoSource::Window(7))
        );
        assert_eq!(
            video_source(&VideoTarget::Display(Some("DISPLAY2".into()))).unwrap(),
            Some(VideoSource::Display(Some("DISPLAY2".into())))
        );
        assert_eq!(
            video_source(&VideoTarget::Display(None)).unwrap(),
            Some(VideoSource::Display(None))
        );
        assert_eq!(video_source(&VideoTarget::Blank).unwrap(), None);
    }

    #[test]
    fn browser_tab_cannot_be_captured() {
        assert!(video_source(&VideoTarget::BrowserTab(BrowserTab { link: 1, tab: 2 })).is_err());
    }

    #[test]
    fn captures_the_app_audio_and_microphone_when_the_target_app_is_known() {
        assert_eq!(
            audio_sources(&request(Some(100)), &AudioConfig::default()),
            [
                AudioSource::ProcessAudio(100),
                AudioSource::Microphone(Some("mic".into()))
            ]
        );
    }

    #[test]
    fn captures_output_device_audio_when_the_target_app_is_unknown() {
        assert_eq!(
            audio_sources(&request(None), &AudioConfig::default()),
            [
                AudioSource::DeviceAudio(Some("speaker".into())),
                AudioSource::Microphone(Some("mic".into()))
            ]
        );
    }

    #[test]
    fn does_not_capture_inputs_disabled_in_settings() {
        let only_microphone = AudioConfig {
            record_system_audio: false,
            record_microphone: true,
        };
        let only_system = AudioConfig {
            record_system_audio: true,
            record_microphone: false,
        };
        let nothing = AudioConfig {
            record_system_audio: false,
            record_microphone: false,
        };

        assert_eq!(
            audio_sources(&request(Some(100)), &only_microphone),
            [AudioSource::Microphone(Some("mic".into()))]
        );
        assert_eq!(
            audio_sources(&request(Some(100)), &only_system),
            [AudioSource::ProcessAudio(100)]
        );
        assert!(audio_sources(&request(Some(100)), &nothing).is_empty());
    }

    #[test]
    fn output_size_is_made_even() {
        assert_eq!(canvas_size(Some((1584, 992))), (1584, 992));
        assert_eq!(canvas_size(Some((1585, 993))), (1584, 992));
    }

    #[test]
    fn a_target_larger_than_1080p_is_written_at_1080p_with_its_aspect_ratio() {
        assert_eq!(canvas_size(Some((3840, 2160))), (1920, 1080));
        assert_eq!(canvas_size(Some((2560, 1440))), (1920, 1080));
        assert_eq!(canvas_size(Some((3440, 1440))), (1920, 804), "ultrawide");
        assert_eq!(
            canvas_size(Some((2160, 3840))),
            (1080, 1920),
            "a tall target keeps its orientation"
        );
        assert_eq!(canvas_size(Some((1200, 1900))), (1080, 1710));
        assert_eq!(canvas_size(Some((1920, 1200))), (1728, 1080));
    }

    #[test]
    fn a_target_within_1080p_is_not_scaled_up() {
        assert_eq!(canvas_size(Some((1920, 1080))), (1920, 1080));
        assert_eq!(canvas_size(Some((1280, 720))), (1280, 720));
        assert_eq!(canvas_size(Some((1080, 1920))), (1080, 1920));
    }

    #[test]
    fn too_small_a_target_is_written_at_the_minimum_size() {
        assert_eq!(canvas_size(Some((160, 28))), (320, 240));
        assert_eq!(canvas_size(Some((1000, 100))), (1000, 240));
    }

    #[test]
    fn starting_without_a_target_writes_at_a_fixed_size() {
        assert_eq!(canvas_size(None), (1280, 720));
    }
}
