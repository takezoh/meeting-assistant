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
        self.scaler = Some(Scaler::new(&self.graphics, format.width, format.height));
        Mp4Writer::create(path, &self.graphics.device, format)
    }

    fn take_picture(&mut self, video: &mut ScreenCapture) -> anyhow::Result<Option<ID3D11Texture2D>> {
        let scaler = self.scaler()?;
        video.take_frame(|frame| scaler.fit(frame))
    }

    fn black_picture(&mut self) -> anyhow::Result<ID3D11Texture2D> {
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
        picture: &ID3D11Texture2D,
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()> {
        writer.write_video(picture, time, duration)
    }

    fn write_audio(&mut self, writer: &mut Mp4Writer, pcm: &[i16], time: i64, duration: i64) -> anyhow::Result<()> {
        writer.write_audio(pcm, time, duration)
    }

    fn finalize(&mut self, writer: Mp4Writer) -> anyhow::Result<()> {
        writer.finalize()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, MSG, PM_REMOVE, PeekMessageW,
        RegisterClassW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, TranslateMessage, WM_PAINT, WNDCLASSW,
        WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::w;

    use super::*;
    use crate::recording::plan;

    const RED: [u8; 4] = [0, 0, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

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

    /// The first picture for which `wanted` holds, waiting up to five seconds.
    fn picture_where(
        backend: &mut WindowsBackend,
        video: &mut ScreenCapture,
        window: &RedWindow,
        wanted: impl Fn(&WindowsBackend, &ID3D11Texture2D) -> bool,
    ) -> ID3D11Texture2D {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            window.pump();
            if let Some(picture) = backend.take_picture(video).unwrap()
                && wanted(backend, &picture)
            {
                return picture;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no such picture arrived");
    }

    /// The BGRA values of `texture` at `points`.
    fn pixels(backend: &WindowsBackend, texture: &ID3D11Texture2D, points: &[(u32, u32)]) -> Vec<[u8; 4]> {
        unsafe {
            let mut description = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut description);
            description.Usage = D3D11_USAGE_STAGING;
            description.BindFlags = 0;
            description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            backend
                .graphics
                .device
                .CreateTexture2D(&description, None, Some(&mut staging))
                .unwrap();
            let staging = staging.unwrap();
            let context = &backend.graphics.context;
            context.CopyResource(&staging, texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
            let values = points
                .iter()
                .map(|&(x, y)| {
                    let at = mapped.pData.cast::<u8>().add((y * mapped.RowPitch + x * 4) as usize);
                    [*at, *at.add(1), *at.add(2), *at.add(3)]
                })
                .collect();
            context.Unmap(&staging, 0);
            values
        }
    }

    fn close_to(actual: [u8; 4], expected: [u8; 4]) -> bool {
        actual.iter().zip(expected).all(|(&a, e)| a.abs_diff(e) <= 8)
    }

    /// Captures a real window, so it needs a desktop session and a graphics adapter:
    /// `cargo test -p meeting-assistant window_pictures -- --ignored`.
    #[test]
    #[ignore = "captures a real window; needs a desktop session"]
    fn window_pictures_are_cropped_when_odd_and_scaled_when_resized_and_written() {
        let window = RedWindow::open(641, 481);
        let mut backend = WindowsBackend::create().unwrap();
        let (mut video, size) = backend.open_video(&VideoSource::Window(window.0.0 as u64)).unwrap();
        assert_eq!(size, (641, 481));
        let (width, height) = plan::canvas_size(Some(size));
        assert_eq!((width, height), (640, 480));
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("window.mp4");
        let format = MediaFormat {
            width,
            height,
            framerate: 30,
            quality: 70,
            fallback_bitrate: 2_000_000,
            has_audio: false,
        };
        let mut writer = backend.open_writer(&path, &format).unwrap();

        // One pixel too large in each direction: cut off, not scaled, so the corners are the window's.
        let corners = [(0, 0), (639, 0), (0, 479), (639, 479)];
        let cropped = picture_where(&mut backend, &mut video, &window, |backend, picture| {
            pixels(backend, picture, &corners)
                .iter()
                .all(|&pixel| close_to(pixel, RED))
        });
        backend.write_video(&mut writer, &cropped, 0, 333_333).unwrap();

        // Wider than the canvas: scaled to 640x240 in the middle, black above and below.
        window.resize(800, 300);
        let column = [(320, 10), (320, 240), (320, 470)];
        let scaled = picture_where(&mut backend, &mut video, &window, |backend, picture| {
            let values = pixels(backend, picture, &column);
            close_to(values[0], BLACK) && close_to(values[1], RED) && close_to(values[2], BLACK)
        });
        backend.write_video(&mut writer, &scaled, 333_333, 333_333).unwrap();

        backend.finalize(writer).unwrap();
        drop(video);
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }
}
