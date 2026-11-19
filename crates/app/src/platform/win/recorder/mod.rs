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
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFStartup};

use self::audio::AudioCapture;
use self::scale::{Picture, PictureFormat, Scaler};
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
    /// The time between two pictures written; capture is asked for no more pictures than that.
    frame_interval: std::time::Duration,
    _com: ComApartment,
}

impl WindowsBackend {
    pub fn create(video: &VideoConfig) -> anyhow::Result<Self> {
        let com = ComApartment::enter()?;
        start_media_foundation()?;
        Ok(Self {
            scaler: None,
            graphics: GraphicsDevice::create()?,
            frame_interval: std::time::Duration::from_secs(1) / video.framerate.max(1),
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
    type Picture = Picture;
    type Writer = Mp4Writer;

    fn open_video(&mut self, source: &VideoSource, _canvas: (u32, u32)) -> anyhow::Result<ScreenCapture> {
        let target = match source {
            VideoSource::Window(window) => CaptureTarget::Window(*window),
            VideoSource::Display(name) => CaptureTarget::Display(name.as_deref()),
        };
        // Each picture is fitted into the canvas by the scaler, whatever the target's size and shape.
        ScreenCapture::start(&self.graphics, target, self.frame_interval)
    }

    fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<AudioCapture> {
        AudioCapture::open(source.clone())
    }

    fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<Mp4Writer> {
        if !format.has_video {
            // No picture is written, so the format pictures would come in does not matter.
            return Mp4Writer::create(path, &self.graphics.device, format, PictureFormat::Nv12);
        }
        let scaler = Scaler::new(&self.graphics, format.width, format.height);
        let writer = Mp4Writer::create(path, &self.graphics.device, format, scaler.format())?;
        self.scaler = Some(scaler);
        Ok(writer)
    }

    fn take_picture(&mut self, video: &mut ScreenCapture) -> anyhow::Result<Option<Picture>> {
        let scaler = self.scaler()?;
        video.take_frame(|frame| scaler.fit(frame))
    }

    fn black_picture(&mut self) -> anyhow::Result<Picture> {
        self.scaler()?.black()
    }

    fn start_writing(&mut self, audio: &mut AudioCapture) {
        audio.start_writing();
    }

    fn mix_audio(&mut self, audio: &mut AudioCapture, out: &mut [i16]) {
        audio.mix_into(out);
    }

    fn write_video(
        &mut self,
        writer: &mut Mp4Writer,
        picture: &Picture,
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT};
    use windows::Win32::Media::MediaFoundation::{
        IMF2DBuffer, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
        MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ALL_STREAMS,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MFAudioFormat_PCM, MFCreateMediaType,
        MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_NV12,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, MSG, PM_REMOVE, PeekMessageW,
        RegisterClassW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, TranslateMessage, WM_PAINT, WNDCLASSW,
        WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::{HSTRING, Interface, w};

    use super::scale::PictureFormat;
    use super::video::Frame;
    use super::*;
    use crate::recording::merge::Placement;
    use crate::recording::mp4;
    use crate::recording::plan::Track;

    fn video_config() -> VideoConfig {
        VideoConfig {
            framerate: 30,
            quality: 70,
            fallback_bitrate_kbps: 2000,
            ..VideoConfig::default()
        }
    }

    /// Luma of pure red and of black in BT.709 with the limited range: 16 + 219 x 0.2126, and 16.
    const RED: u8 = 63;
    const BLACK: u8 = 16;

    /// A borderless window filled with red apart from its last column and row, which are blue, sized in physical
    /// pixels. Cropping the extra pixel of an odd size removes the blue; scaling would blend it into the picture.
    struct RedWindow(HWND);

    impl RedWindow {
        fn open(width: i32, height: i32) -> Self {
            unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
                unsafe {
                    if message == WM_PAINT {
                        let mut paint = PAINTSTRUCT::default();
                        let context = BeginPaint(window, &mut paint);
                        let mut area = RECT::default();
                        let _ = GetClientRect(window, &mut area);
                        let blue = CreateSolidBrush(COLORREF(0x00FF_0000));
                        FillRect(
                            context,
                            &RECT {
                                left: area.right - 1,
                                ..area
                            },
                            blue,
                        );
                        FillRect(
                            context,
                            &RECT {
                                top: area.bottom - 1,
                                ..area
                            },
                            blue,
                        );
                        let _ = DeleteObject(blue.into());
                        let _ = EndPaint(window, &paint);
                        return LRESULT(0);
                    }
                    DefWindowProcW(window, message, wparam, lparam)
                }
            }
            unsafe {
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                let instance = GetModuleHandleW(None).unwrap();
                let class = w!("meeting-assistant-test-red-window");
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    hbrBackground: CreateSolidBrush(COLORREF(0x0000_00FF)),
                    ..Default::default()
                });
                let window = CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    class,
                    w!("test"),
                    WS_POPUP | WS_VISIBLE,
                    100,
                    100,
                    width,
                    height,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .unwrap();
                let opened = Self(window);
                opened.pump();
                opened
            }
        }

        fn resize(&self, width: i32, height: i32) {
            unsafe {
                SetWindowPos(
                    self.0,
                    None,
                    0,
                    0,
                    width,
                    height,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .unwrap();
        }

        fn pump(&self) {
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }

    impl Drop for RedWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    /// The first picture whose luma at `points` is as `wanted`, waiting up to five seconds.
    fn picture_where(
        backend: &mut WindowsBackend,
        video: &mut ScreenCapture,
        window: &RedWindow,
        points: &[(u32, u32)],
        wanted: impl Fn(&[u8]) -> bool,
    ) -> Picture {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = None;
        while Instant::now() < deadline {
            window.pump();
            if let Some(picture) = backend.take_picture(video).unwrap() {
                let values = luma(&backend.graphics, picture.texture(), points);
                if wanted(&values) {
                    return picture;
                }
                seen = Some(values);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no such picture arrived; the last had luma {seen:?} at {points:?}");
    }

    /// The luma of an NV12 `texture` at `points`.
    fn luma(graphics: &GraphicsDevice, texture: &ID3D11Texture2D, points: &[(u32, u32)]) -> Vec<u8> {
        unsafe {
            let mut description = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut description);
            description.Usage = D3D11_USAGE_STAGING;
            description.BindFlags = 0;
            description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            graphics
                .device
                .CreateTexture2D(&description, None, Some(&mut staging))
                .unwrap();
            let staging = staging.unwrap();
            graphics.context.CopyResource(&staging, texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            graphics
                .context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .unwrap();
            // The luma plane comes first, one byte per pixel.
            let values = points
                .iter()
                .map(|&(x, y)| *mapped.pData.cast::<u8>().add((y * mapped.RowPitch + x) as usize))
                .collect();
            graphics.context.Unmap(&staging, 0);
            values
        }
    }

    fn close_to(actual: u8, expected: u8) -> bool {
        actual.abs_diff(expected) <= 4
    }

    /// A BGRA texture of one color, as a capture would hand it over.
    fn solid_frame(graphics: &GraphicsDevice, width: u32, height: u32, bgra: [u8; 4]) -> Frame {
        let pixels: Vec<u8> = std::iter::repeat_n(bgra, (width * height) as usize).flatten().collect();
        let description = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DEFAULT,
            // As the capture's own surfaces: the video processor reads only textures it could also render to.
            BindFlags: (windows::Win32::Graphics::Direct3D11::D3D11_BIND_RENDER_TARGET.0
                | windows::Win32::Graphics::Direct3D11::D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let data = D3D11_SUBRESOURCE_DATA {
            pSysMem: pixels.as_ptr().cast(),
            SysMemPitch: width * 4,
            SysMemSlicePitch: 0,
        };
        let mut texture = None;
        unsafe {
            graphics
                .device
                .CreateTexture2D(&description, Some(&data), Some(&mut texture))
                .unwrap()
        };
        Frame {
            texture: texture.unwrap(),
            width,
            height,
        }
    }

    /// Needs a graphics adapter and Media Foundation's encoders:
    /// `cargo test -p meeting-assistant pictures_in_nv12 -- --ignored --nocapture`.
    #[test]
    #[ignore = "uses the graphics adapter and the system's H.264 encoders"]
    fn pictures_in_nv12_are_encoded_by_the_gpus_encoder_and_by_the_software_encoder() {
        let backend = WindowsBackend::create(&video_config()).unwrap();
        let format = MediaFormat {
            width: 640,
            height: 360,
            framerate: 30,
            quality: 70,
            fallback_bitrate: 2_000_000,
            has_video: true,
            audio_tracks: Vec::new(),
        };
        let mut scaler = Scaler::new(&backend.graphics, format.width, format.height);
        assert_eq!(scaler.format(), PictureFormat::Nv12);

        // A 1280x720 red capture, scaled down by half in the conversion.
        let red = scaler
            .fit(&solid_frame(&backend.graphics, 1280, 720, [0, 0, 255, 255]))
            .unwrap();
        assert!(
            luma(&backend.graphics, red.texture(), &[(0, 0), (320, 180), (639, 359)])
                .iter()
                .all(|&value| close_to(value, RED))
        );
        let black = scaler.black().unwrap();
        assert!(
            luma(&backend.graphics, black.texture(), &[(0, 0), (639, 359)])
                .iter()
                .all(|&value| close_to(value, BLACK))
        );

        let folder = tempfile::tempdir().unwrap();
        for in_software in [false, true] {
            let path = folder.path().join(format!("software-{in_software}.mp4"));
            let writer = if in_software {
                Mp4Writer::create_in_software(&path, &backend.graphics.device, &format, PictureFormat::Nv12)
            } else {
                Mp4Writer::create(&path, &backend.graphics.device, &format, PictureFormat::Nv12)
            }
            .unwrap();
            println!("software {in_software}: {}", writer.encoder());
            assert!(writer.encoder().contains("encoder"), "{}", writer.encoder());
            assert!(writer.encoder().contains("takes NV12"), "{}", writer.encoder());
            assert_eq!(
                writer.encoder().contains("software"),
                in_software,
                "{}",
                writer.encoder()
            );
            for index in 0..30i64 {
                let picture = if index % 2 == 0 { &red } else { &black };
                writer.write_video(picture, index * 333_333, 333_333).unwrap();
            }
            writer.finalize().unwrap();
            assert!(std::fs::metadata(&path).unwrap().len() > 0);
        }
    }

    /// A tone of `hz` on one channel from `onset` (in audio frames at 48 kHz) and silence before, as 16-bit stereo:
    /// `frames` frames from frame `from`.
    fn tone_from(onset: usize, hz: f64, left: bool, from: usize, frames: usize) -> Vec<i16> {
        let mut pcm = Vec::with_capacity(frames * 2);
        for index in from..from + frames {
            let value = if index < onset {
                0
            } else {
                let seconds = (index - onset) as f64 / 48_000.0;
                ((seconds * hz * std::f64::consts::TAU).sin() * 8000.0) as i16
            };
            pcm.extend_from_slice(&if left { [value, 0] } else { [0, value] });
        }
        pcm
    }

    /// The sound of each audio stream of the file at `path`, as Media Foundation decodes it to 48 kHz stereo 16-bit:
    /// when its first sample plays, in 100 ns units, and the samples. They come in Media Foundation's order, which is
    /// the reverse of the file's.
    fn decoded_sounds(path: &Path) -> Vec<(i64, Vec<i16>)> {
        unsafe {
            let open = || MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).unwrap();
            let reader = open();
            let audio: Vec<u32> = (0..)
                .map_while(|index| Some((index, reader.GetNativeMediaType(index, 0).ok()?)))
                .filter(|(_, media_type)| media_type.GetGUID(&MF_MT_MAJOR_TYPE).unwrap() == MFMediaType_Audio)
                .map(|(index, _)| index)
                .collect();
            let mut sounds = Vec::new();
            for stream in audio {
                let reader = open();
                reader
                    .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                    .unwrap();
                reader.SetStreamSelection(stream, true).unwrap();
                let wanted = MFCreateMediaType().unwrap();
                wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).unwrap();
                wanted.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).unwrap();
                wanted.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).unwrap();
                wanted.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000).unwrap();
                wanted.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).unwrap();
                reader.SetCurrentMediaType(stream, None, &wanted).unwrap();
                let mut first = None;
                let mut pcm = Vec::new();
                loop {
                    let mut flags = 0u32;
                    let mut time = 0i64;
                    let mut sample = None;
                    reader
                        .ReadSample(stream, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample))
                        .unwrap();
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        break;
                    }
                    let Some(sample) = sample else { continue };
                    first.get_or_insert(time);
                    let buffer = sample.ConvertToContiguousBuffer().unwrap();
                    let mut data = std::ptr::null_mut();
                    let mut length = 0u32;
                    buffer.Lock(&mut data, None, Some(&mut length)).unwrap();
                    pcm.extend_from_slice(std::slice::from_raw_parts(data.cast::<i16>(), length as usize / 2));
                    buffer.Unlock().unwrap();
                }
                sounds.push((first.expect("a stream has no sound"), pcm));
            }
            sounds
        }
    }

    /// The loudest sample on one channel of `pcm`.
    fn peak(pcm: &[i16], left: bool) -> u16 {
        let channel = usize::from(!left);
        pcm.chunks(2)
            .map(|frame| frame[channel].unsigned_abs())
            .max()
            .unwrap_or(0)
    }

    /// When the sound on one channel first gets loud, in milliseconds, from when the stream's first sample plays.
    fn onset_milliseconds(first: i64, pcm: &[i16], left: bool) -> f64 {
        let channel = usize::from(!left);
        let frame = pcm
            .chunks(2)
            .position(|frame| frame[channel].unsigned_abs() > 2000)
            .expect("the channel stays quiet");
        first as f64 / 10_000.0 + frame as f64 * 1000.0 / 48_000.0
    }

    /// Needs Media Foundation's H.264 and AAC encoders and its decoders:
    /// `cargo test -p meeting-assistant a_microphone_recorded_here -- --ignored --nocapture`.
    #[test]
    #[ignore = "uses the system's encoders and decoders"]
    fn a_microphone_recorded_here_is_added_to_a_recording_where_it_was_heard() {
        let backend = WindowsBackend::create(&video_config()).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let (recording, microphone, merged) = (
            folder.path().join("recording.mp4"),
            folder.path().join("recording.microphone.m4a"),
            folder.path().join("merged.mp4"),
        );
        // The recording's other side sounds on the left from 1.5 s. The microphone, started 0.5 s after the
        // recording, sounds on the right from 1.0 s of its own, which is the same moment.
        let mut scaler = Scaler::new(&backend.graphics, 640, 360);
        let black = scaler.black().unwrap();
        for (path, has_video, track, onset, hz, left) in [
            (&recording, true, Track::OtherSide, 72_000, 440.0, true),
            (&microphone, false, Track::Microphone, 48_000, 880.0, false),
        ] {
            let format = MediaFormat {
                width: 640,
                height: 360,
                framerate: 30,
                quality: 70,
                fallback_bitrate: 2_000_000,
                has_video,
                audio_tracks: vec![track],
            };
            let writer = Mp4Writer::create(path, &backend.graphics.device, &format, PictureFormat::Nv12).unwrap();
            // Three seconds, a frame (and 1600 frames of sound) at a time.
            for step in 0..90usize {
                let time = step as i64 * 333_333;
                writer.write_video(&black, time, 333_333).unwrap();
                let pcm = tone_from(onset, hz, left, step * 1600, 1600);
                writer.write_audio(0, &pcm, time, 333_333).unwrap();
            }
            writer.finalize().unwrap();
        }

        let (recording_bytes, microphone_bytes) =
            (std::fs::read(&recording).unwrap(), std::fs::read(&microphone).unwrap());
        let placement = Placement {
            skip: Duration::ZERO,
            delay: Duration::from_millis(500),
        };
        let pieces = mp4::add_audio_track(&recording_bytes, &microphone_bytes, placement).unwrap();
        let mut out = std::fs::File::create(&merged).unwrap();
        mp4::write_pieces(&pieces, &recording_bytes, &microphone_bytes, &mut out).unwrap();
        drop(out);

        let sounds = decoded_sounds(&merged);
        assert_eq!(sounds.len(), 2);
        let track = |left: bool| {
            sounds
                .iter()
                .find(|(_, pcm)| peak(pcm, left) > 4000)
                .expect("a track with that channel")
        };
        let (other_side, voice) = (track(true), track(false));
        assert!(
            peak(&other_side.1, false) < 500 && peak(&voice.1, true) < 500,
            "each track has only its own sound"
        );
        let heard = onset_milliseconds(other_side.0, &other_side.1, true);
        let spoken = onset_milliseconds(voice.0, &voice.1, false);
        println!(
            "other side from {} ms, loud at {heard:.1} ms; microphone from {} ms, loud at {spoken:.1} ms",
            other_side.0 / 10_000,
            voice.0 / 10_000
        );
        assert!((voice.0 - 5_000_000).abs() <= 10_000, "the microphone starts 0.5 s in");
        assert!((heard - spoken).abs() <= 3.0, "{heard} ms against {spoken} ms");
        assert!((heard - 1500.0).abs() <= 50.0, "{heard} ms");
    }

    /// Needs a graphics adapter: `cargo test -p meeting-assistant a_picture_smaller -- --ignored`.
    #[test]
    #[ignore = "uses the graphics adapter"]
    fn a_picture_smaller_than_the_canvas_is_drawn_in_its_middle_at_its_own_size() {
        let backend = WindowsBackend::create(&video_config()).unwrap();
        let mut scaler = Scaler::new(&backend.graphics, 640, 360);

        // 320x180 in the middle of 640x360 covers columns 160 to 479 and rows 90 to 269.
        let red = scaler
            .fit(&solid_frame(&backend.graphics, 320, 180, [0, 0, 255, 255]))
            .unwrap();
        let inside = luma(
            &backend.graphics,
            red.texture(),
            &[(160, 90), (479, 90), (160, 269), (479, 269), (320, 180)],
        );
        let outside = luma(
            &backend.graphics,
            red.texture(),
            &[(159, 180), (480, 180), (320, 89), (320, 270), (0, 0), (639, 359)],
        );
        assert!(inside.iter().all(|&value| close_to(value, RED)), "{inside:?}");
        assert!(outside.iter().all(|&value| close_to(value, BLACK)), "{outside:?}");
    }

    /// The luma in the middle of each picture of the video at `path`, as decoded by Media Foundation.
    fn middle_luma_of_each_picture(path: &Path) -> Vec<u8> {
        unsafe {
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).unwrap();
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let wanted = MFCreateMediaType().unwrap();
            wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).unwrap();
            wanted.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).unwrap();
            reader.SetCurrentMediaType(stream, None, &wanted).unwrap();
            let size = reader
                .GetCurrentMediaType(stream)
                .unwrap()
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .unwrap();
            let (width, height) = ((size >> 32) as isize, (size & 0xffff_ffff) as isize);
            let mut values = Vec::new();
            loop {
                let mut flags = 0u32;
                let mut sample = None;
                reader
                    .ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))
                    .unwrap();
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                let Some(sample) = sample else { continue };
                let buffer: IMF2DBuffer = sample.ConvertToContiguousBuffer().unwrap().cast().unwrap();
                let mut first_row = std::ptr::null_mut();
                let mut pitch = 0;
                buffer.Lock2D(&mut first_row, &mut pitch).unwrap();
                values.push(*first_row.offset(height / 2 * pitch as isize + width / 2));
                buffer.Unlock2D().unwrap();
            }
            values
        }
    }

    /// Needs a graphics adapter and Media Foundation's encoders:
    /// `cargo test -p meeting-assistant pictures_are_drawn -- --ignored --nocapture`.
    #[test]
    #[ignore = "uses the graphics adapter and the system's H.264 encoders"]
    fn pictures_are_drawn_into_a_texture_again_only_once_the_encoder_has_let_go_of_it() {
        const PICTURES: usize = 24;
        const REPEATS: usize = 3;
        let backend = WindowsBackend::create(&video_config()).unwrap();
        let format = MediaFormat {
            width: 640,
            height: 360,
            framerate: 30,
            quality: 70,
            fallback_bitrate: 2_000_000,
            has_video: true,
            audio_tracks: Vec::new(),
        };
        // Colors far apart in luma: black, red, green (16 + 219 x 0.7152) and white.
        let colors = [
            (solid_frame(&backend.graphics, 640, 360, [0, 0, 0, 255]), BLACK),
            (solid_frame(&backend.graphics, 640, 360, [0, 0, 255, 255]), RED),
            (solid_frame(&backend.graphics, 640, 360, [0, 255, 0, 255]), 173),
            (solid_frame(&backend.graphics, 640, 360, [255, 255, 255, 255]), 235),
        ];
        let folder = tempfile::tempdir().unwrap();
        // Written as fast as possible, the encoder holds many samples at once, and a texture drawn into while it is
        // held shows in the file. Written once a frame, as when recording, the encoder lets go of each in time.
        for (in_software, paced) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut scaler = Scaler::new(&backend.graphics, format.width, format.height);
            let path = folder.path().join(format!("software-{in_software}-paced-{paced}.mp4"));
            let writer = if in_software {
                Mp4Writer::create_in_software(&path, &backend.graphics.device, &format, PictureFormat::Nv12)
            } else {
                Mp4Writer::create(&path, &backend.graphics.device, &format, PictureFormat::Nv12)
            }
            .unwrap();

            // As the pipeline does: each new picture is made while the one before is still held, then written until
            // the next arrives.
            let mut expected = Vec::new();
            let mut held = None;
            for index in 0..PICTURES {
                let (frame, luma) = &colors[index % colors.len()];
                held = Some(scaler.fit(frame).unwrap());
                for _ in 0..REPEATS {
                    let time = expected.len() as i64 * 333_333;
                    writer.write_video(held.as_ref().unwrap(), time, 333_333).unwrap();
                    expected.push(*luma);
                    if paced {
                        std::thread::sleep(Duration::from_micros(33_333));
                    }
                }
            }
            drop(held);
            writer.finalize().unwrap();

            // The encoder lets go of its samples on a work queue thread.
            let (textures, _) = scaler.textures();
            let deadline = Instant::now() + Duration::from_secs(5);
            while scaler.textures().1 < textures && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("software {in_software}, paced {paced}: {PICTURES} pictures drawn into {textures} textures");
            assert_eq!(
                scaler.textures(),
                (textures, textures),
                "every texture is let go of in the end"
            );
            if paced {
                assert!(textures <= 3, "textures are drawn into again");
            }

            let decoded = middle_luma_of_each_picture(&path);
            assert_eq!(decoded.len(), expected.len());
            for (index, (&actual, &wanted)) in decoded.iter().zip(&expected).enumerate() {
                assert!(
                    actual.abs_diff(wanted) <= 12,
                    "picture {index} has luma {actual} instead of {wanted}: {decoded:?}"
                );
            }
        }
    }

    /// Changes a real window over and over, so it needs a desktop session:
    /// `cargo test -p meeting-assistant capture_sends -- --ignored --nocapture`.
    #[test]
    #[ignore = "captures a real window; needs a desktop session"]
    fn capture_sends_no_more_pictures_than_one_per_interval() {
        let window = RedWindow::open(300, 300);
        let backend = WindowsBackend::create(&video_config()).unwrap();
        let mut arrived = Vec::new();
        for interval in [Duration::from_millis(1), Duration::from_millis(400)] {
            let target = CaptureTarget::Window(window.0.0 as u64);
            let capture = ScreenCapture::start(&backend.graphics, target, interval).unwrap();
            let started = Instant::now();
            let mut wider = false;
            while started.elapsed() < Duration::from_secs(2) {
                wider = !wider;
                window.resize(300 + i32::from(wider), 300);
                window.pump();
                std::thread::sleep(Duration::from_millis(4));
            }
            arrived.push(capture.arrived());
        }
        println!(
            "pictures sent in 2 s: {} with one asked for every 1 ms, {} with one every 400 ms",
            arrived[0], arrived[1]
        );
        assert!(arrived[1] <= 7, "{arrived:?}");
        assert!(arrived[0] > 2 * arrived[1], "{arrived:?}");
    }

    /// Captures a real window, so it needs a desktop session and a graphics adapter:
    /// `cargo test -p meeting-assistant window_pictures -- --ignored`.
    #[test]
    #[ignore = "captures a real window; needs a desktop session"]
    fn window_pictures_are_cropped_when_odd_and_scaled_when_resized_and_written() {
        let window = RedWindow::open(641, 481);
        let mut backend = WindowsBackend::create(&video_config()).unwrap();
        // A canvas one pixel smaller in each direction than the window.
        let (width, height) = (640, 480);
        let mut video = backend
            .open_video(&VideoSource::Window(window.0.0 as u64), (width, height))
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("window.mp4");
        let format = MediaFormat {
            width,
            height,
            framerate: 30,
            quality: 70,
            fallback_bitrate: 2_000_000,
            has_video: true,
            audio_tracks: Vec::new(),
        };
        let mut writer = backend.open_writer(&path, &format).unwrap();

        // One pixel too large in each direction: cut off, not scaled, so the corners are the window's red, with none
        // of the blue last column and row blended in.
        let corners = [(0, 0), (639, 0), (0, 479), (639, 479)];
        let cropped = picture_where(&mut backend, &mut video, &window, &corners, |values| {
            values.iter().all(|&value| close_to(value, RED))
        });
        backend.write_video(&mut writer, &cropped, 0, 333_333).unwrap();

        // Wider than the canvas: scaled to 640x240 in the middle, black above and below.
        window.resize(800, 300);
        let column = [(320, 10), (320, 240), (320, 470)];
        let scaled = picture_where(&mut backend, &mut video, &window, &column, |values| {
            close_to(values[0], BLACK) && close_to(values[1], RED) && close_to(values[2], BLACK)
        });
        backend.write_video(&mut writer, &scaled, 333_333, 333_333).unwrap();

        backend.finalize(writer).unwrap();
        drop(video);
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }
}
