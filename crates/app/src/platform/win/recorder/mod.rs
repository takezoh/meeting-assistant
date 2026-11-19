//! Capture and writing on Windows.
//!
//! How a recording proceeds (what to capture, when to write, how to mix audio, thread management) lives in
//! `crate::recording`; this module only implements its port, [`CaptureBackend`].
//!
//! | Operation | Implementation |
//! |---|---|
//! | Video capture | Windows.Graphics.Capture (window or display) |
//! | Size fitting | Direct3D 11 video processor |
//! | Audio capture | WASAPI (app audio, output device audio, microphone) |
//! | Writing | Media Foundation (H.264/AAC fragmented MP4) |

mod audio;
mod scale;
mod video;
mod writer;

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Context;
use meeting_assistant_core::{AudioConfig, VideoConfig};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFStartup};

use self::audio::AudioCapture;
use self::scale::Scaler;
use self::video::{CaptureTarget, GraphicsDevice, ScreenCapture};
use self::writer::Mp4Writer;
use super::com::ComApartment;
use crate::recording::pipeline::CaptureBackend;
use crate::recording::plan::{AudioSource, MediaFormat, VideoSource};
use crate::recording::session::ThreadedRecorder;

pub type PlatformRecorder = ThreadedRecorder<WindowsBackend>;

/// Creates the recorder.
pub fn recorder(video: VideoConfig, audio: AudioConfig) -> PlatformRecorder {
    ThreadedRecorder::new(video, audio, WindowsBackend::create)
}

/// Starts Media Foundation once per process.
/// Starting and shutting it down for every recording leaves handles behind each time,
/// so shutdown is left to process exit.
fn start_media_foundation() -> anyhow::Result<()> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|e| e.to_string()))
        .clone()
        .map_err(|error| anyhow::anyhow!("cannot start Media Foundation: {error}"))
}

/// Windows-side tools for one recording. Created inside the recording thread and used on that thread.
pub struct WindowsBackend {
    // Release the COM objects used for capture and writing first, then leave COM
    // (fields are dropped in declaration order).
    scaler: Option<Scaler>,
    graphics: GraphicsDevice,
    _com: ComApartment,
}

impl WindowsBackend {
    pub fn create() -> anyhow::Result<Self> {
        let com = ComApartment::enter()?;
        start_media_foundation()?;
        Ok(Self {
            scaler: None,
            graphics: GraphicsDevice::create()?,
            _com: com,
        })
    }

    fn scaler(&mut self) -> anyhow::Result<&mut Scaler> {
        self.scaler
            .as_mut()
            .context("tried to handle video before the output was opened")
    }
}

impl CaptureBackend for WindowsBackend {
    type Video = ScreenCapture;
    type Audio = AudioCapture;
    type Picture = ID3D11Texture2D;
    type Writer = Mp4Writer;

    fn open_video(&mut self, source: &VideoSource) -> anyhow::Result<(ScreenCapture, (u32, u32))> {
        let target = match source {
            VideoSource::Window(window) => CaptureTarget::Window(*window),
            VideoSource::Display(name) => CaptureTarget::Display(name.as_deref()),
        };
        let capture = ScreenCapture::start(&self.graphics, target)?;
        let size = capture.initial_size();
        Ok((capture, size))
    }

    fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<AudioCapture> {
        AudioCapture::open(source.clone())
    }

    fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<Mp4Writer> {
        if format.has_video {
            self.scaler = Some(Scaler::new(&self.graphics, format.width, format.height));
        }
        Mp4Writer::create(path, &self.graphics.device, format)
    }

    fn take_picture(&mut self, video: &mut ScreenCapture) -> anyhow::Result<Option<ID3D11Texture2D>> {
        match video.take_frame()? {
            Some(frame) => Ok(Some(self.scaler()?.fit(&frame)?)),
            None => Ok(None),
        }
    }

    fn black_picture(&mut self) -> anyhow::Result<ID3D11Texture2D> {
        self.scaler()?.black()
    }

    fn mix_audio(&mut self, audio: &mut AudioCapture, out: &mut [i16]) {
        audio.mix_into(out);
    }

    fn write_video(
        &mut self,
        writer: &mut Mp4Writer,
        picture: &ID3D11Texture2D,
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()> {
        writer.write_video(picture, time, duration)
    }

    fn write_audio(
        &mut self,
        writer: &mut Mp4Writer,
        track: usize,
        pcm: &[i16],
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()> {
        writer.write_audio(track, pcm, time, duration)
    }

    fn finalize(&mut self, writer: Mp4Writer) -> anyhow::Result<()> {
        writer.finalize()
    }
}
