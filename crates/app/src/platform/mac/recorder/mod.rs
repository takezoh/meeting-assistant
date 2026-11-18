//! Capture and writing on macOS.
//!
//! How a recording proceeds (what to capture, when to write, how to mix audio, thread management) lives in
//! `crate::recording`; this module only implements its port, [`CaptureBackend`].
//!
//! | Operation | Implementation |
//! |---|---|
//! | Video capture and size fitting | ScreenCaptureKit (a window or a display, scaled into the canvas by the system) |
//! | App audio, all output audio | ScreenCaptureKit audio of a filter that includes one app, or every app |
//! | Microphone | AVAudioEngine input node |
//! | Writing | AVAssetWriter (H.264/AAC MP4 with movie fragments) |

mod content;
mod microphone;
mod stream;
mod writer;

use std::path::Path;
use std::ptr::NonNull;

use anyhow::Context;
use meeting_assistant_core::{AudioConfig, VideoConfig};
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};

use self::content::{Content, application_filter, display_filter, pixel_size, window_filter};
pub use self::microphone::Microphone;
use self::stream::{Capture, Kind, PIXEL_FORMAT, Picture};
use self::writer::Mp4Writer;
use crate::recording::pipeline::CaptureBackend;
use crate::recording::plan::{self, AudioSource, MediaFormat, VideoSource};
use crate::recording::session::ThreadedRecorder;

/// The most pictures per second asked of ScreenCaptureKit. It sends a picture only when something changed, and the
/// pipeline takes them at the configured frame rate.
const MAX_FRAMERATE: u32 = 60;

pub type PlatformRecorder = ThreadedRecorder<MacBackend>;

/// Creates the recorder.
pub fn recorder(video: VideoConfig, audio: AudioConfig) -> PlatformRecorder {
    ThreadedRecorder::new(video, audio, MacBackend::create)
}

/// macOS-side tools for one recording. Created inside the recording thread and used on that thread.
pub struct MacBackend {
    /// The size pictures are written at, once the output is open; later captures are fitted into it.
    canvas: Option<(u32, u32)>,
    black: Option<Picture>,
}

impl MacBackend {
    pub fn create() -> anyhow::Result<Self> {
        Ok(Self {
            canvas: None,
            black: None,
        })
    }
}

/// A picture capture, and whether its target is a window (which may close) or a display.
pub struct Video {
    capture: Capture,
    is_window: bool,
}

pub enum Audio {
    Stream(Capture),
    Microphone(Microphone),
}

impl CaptureBackend for MacBackend {
    type Video = Video;
    type Audio = Audio;
    type Picture = Picture;
    type Writer = Mp4Writer;

    fn open_video(&mut self, source: &VideoSource) -> anyhow::Result<(Video, (u32, u32))> {
        let content = Content::current()?;
        let filter = match source {
            VideoSource::Window(window) => window_filter(&*content.window(*window)?),
            VideoSource::Display(display) => display_filter(&*content.display(display.as_deref())?),
        };
        let source_size = pixel_size(&filter);
        // Before the output exists the canvas is what the pipeline will choose for this source.
        let size = self.canvas.unwrap_or_else(|| plan::canvas_size(Some(source_size)));
        let capture = Capture::start(
            &filter,
            &Kind::Pictures {
                size,
                framerate: MAX_FRAMERATE,
            },
        )?;
        Ok((
            Video {
                capture,
                is_window: source.is_window(),
            },
            source_size,
        ))
    }

    fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<Audio> {
        match source {
            AudioSource::ProcessAudio(process_id) => {
                let content = Content::current()?;
                let filter = application_filter(&*content.display(None)?, &*content.application(*process_id)?);
                Ok(Audio::Stream(Capture::start(&filter, &Kind::Sound)?))
            }
            // ScreenCaptureKit captures what every app plays, whichever device it plays to.
            AudioSource::DeviceAudio(_) => {
                let content = Content::current()?;
                let filter = display_filter(&*content.display(None)?);
                Ok(Audio::Stream(Capture::start(&filter, &Kind::Sound)?))
            }
            AudioSource::Microphone(_) => Ok(Audio::Microphone(Microphone::start()?)),
        }
    }

    fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<Mp4Writer> {
        self.canvas = Some((format.width, format.height));
        Mp4Writer::create(path, format)
    }

    fn take_picture(&mut self, video: &mut Video) -> anyhow::Result<Option<Picture>> {
        if let Some(reason) = video.capture.stopped() {
            // A closed window has no new picture; the engine moves the recording to another window of the app.
            anyhow::ensure!(video.is_window, "the display being captured is gone: {reason}");
            return Ok(None);
        }
        Ok(video.capture.take_picture())
    }

    fn black_picture(&mut self) -> anyhow::Result<Picture> {
        if let Some(black) = &self.black {
            return Ok(black.clone());
        }
        let (width, height) = self
            .canvas
            .context("tried to make a picture before the output was opened")?;
        let black = black_picture(width, height)?;
        self.black = Some(black.clone());
        Ok(black)
    }

    fn mix_audio(&mut self, audio: &mut Audio, out: &mut [i16]) {
        match audio {
            Audio::Stream(capture) => capture.mix_into(out),
            Audio::Microphone(microphone) => microphone.mix_into(out),
        }
    }

    fn write_video(
        &mut self,
        writer: &mut Mp4Writer,
        picture: &Picture,
        time: i64,
        _duration: i64,
    ) -> anyhow::Result<()> {
        writer.write_video(&picture.0, time)
    }

    fn write_audio(&mut self, writer: &mut Mp4Writer, pcm: &[i16], time: i64, _duration: i64) -> anyhow::Result<()> {
        writer.write_audio(pcm, time)
    }

    fn finalize(&mut self, writer: Mp4Writer) -> anyhow::Result<()> {
        writer.finalize()
    }
}

/// A black picture in the capture's pixel format: luma at 16 and chroma at 128 (video range).
fn black_picture(width: u32, height: u32) -> anyhow::Result<Picture> {
    let mut buffer: *mut CVPixelBuffer = std::ptr::null_mut();
    // SAFETY: Core Video allocates the buffer and writes its pointer.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            width as usize,
            height as usize,
            PIXEL_FORMAT,
            None,
            NonNull::from(&mut buffer),
        )
    };
    anyhow::ensure!(
        status == 0 && !buffer.is_null(),
        "cannot make a black picture ({status})"
    );
    // SAFETY: a +1 reference from the call above.
    let buffer = unsafe { CFRetained::from_raw(NonNull::new_unchecked(buffer)) };
    // SAFETY: the buffer is locked while its planes are written, each within its bytes per row and height.
    unsafe {
        anyhow::ensure!(
            CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) == 0,
            "cannot write the black picture"
        );
        for (plane, value) in [(0, 16u8), (1, 128u8)] {
            let base = CVPixelBufferGetBaseAddressOfPlane(&buffer, plane);
            let length =
                CVPixelBufferGetBytesPerRowOfPlane(&buffer, plane) * CVPixelBufferGetHeightOfPlane(&buffer, plane);
            if !base.is_null() {
                std::ptr::write_bytes(base.cast::<u8>(), value, length);
            }
        }
        CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
    }
    Ok(Picture(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes two seconds of black pictures and a tone through AVAssetWriter, without capturing anything (no
    /// permission is needed), and checks that the file is finished.
    #[test]
    fn the_writer_finishes_an_mp4_with_video_and_audio() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("written.mp4");
        let mut backend = MacBackend::create().unwrap();
        let format = MediaFormat {
            width: 640,
            height: 360,
            framerate: 30,
            quality: 70,
            fallback_bitrate: 2_000_000,
            has_audio: true,
        };
        let mut writer = backend.open_writer(&path, &format).unwrap();
        let black = backend.black_picture().unwrap();

        let frames_per_picture = plan::SAMPLE_RATE as usize / 30;
        for index in 0..60i64 {
            let time = index * 10_000_000 / 30;
            backend.write_video(&mut writer, &black, time, 0).unwrap();
            let tone: Vec<f32> = (0..frames_per_picture)
                .map(|i| ((index as usize * frames_per_picture + i) as f32 * 0.06).sin() * 0.3)
                .collect();
            let pcm = crate::recording::pcm::interleave_f32_to_i16(&[&tone], plan::CHANNELS);
            backend.write_audio(&mut writer, &pcm, time, 0).unwrap();
            // Real time, so the encoder is ready for the next picture as it would be while recording.
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        backend.finalize(writer).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        if let Ok(keep) = std::env::var("MEETING_ASSISTANT_KEEP_TEST_MP4") {
            std::fs::copy(&path, keep).unwrap();
        }
    }
}
