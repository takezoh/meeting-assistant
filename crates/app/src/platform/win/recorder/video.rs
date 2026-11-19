//! Captures one window or one display with Windows.Graphics.Capture.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice,
    ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::{BOOL, Interface, Ref};

use super::super::signals::display_name;

const PIXEL_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
/// The buffers the capture draws into. One holds the latest picture until it is taken, the other receives the next.
/// Each is as large as the target (33 MB for a 4K display), so no more are kept.
pub const CAPTURE_BUFFERS: usize = 2;
const POOL_BUFFERS: i32 = CAPTURE_BUFFERS as i32;

/// The Direct3D device shared by capture, scaling and encoding.
pub struct GraphicsDevice {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
}

impl GraphicsDevice {
    pub fn create() -> anyhow::Result<Self> {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .context("cannot create the graphics device")?;
            let (device, context): (ID3D11Device, ID3D11DeviceContext) = (device.unwrap(), context.unwrap());
            // Because capture, duplication and the encoder use the same device from different threads.
            let _ = context.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
            if let Ok(description) = device
                .cast::<IDXGIDevice>()
                .and_then(|device| device.GetAdapter())
                .and_then(|adapter| adapter.GetDesc())
            {
                let name = &description.Description;
                let length = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                log::info!("graphics adapter: {}", String::from_utf16_lossy(&name[..length]));
            }
            Ok(Self { device, context })
        }
    }

    /// Creates a BGRA texture that can be handed to the encoder or the scaler.
    pub fn create_texture(&self, width: u32, height: u32) -> anyhow::Result<ID3D11Texture2D> {
        let description = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        unsafe { self.device.CreateTexture2D(&description, None, Some(&mut texture))? };
        texture.context("cannot create the texture")
    }
}

/// The capture target.
#[derive(Debug, Clone, Copy)]
pub enum CaptureTarget<'a> {
    /// A single window (the value of `WindowEntry::id`).
    Window(u64),
    /// A single display (in `\\.\DISPLAY1` form). `None`, or a display that cannot be found, means the main display.
    Display(Option<&'a str>),
}

/// One captured picture. Its texture belongs to the capture and is reused once the picture has been handled, so
/// whoever handles it copies what it needs.
pub struct Frame {
    pub texture: ID3D11Texture2D,
    /// The part of the texture that holds the picture.
    pub width: u32,
    pub height: u32,
}

/// The capture. Keeps only the latest of the arriving pictures.
pub struct ScreenCapture {
    winrt_device: IDirect3DDevice,
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_arrived: i64,
    item_closed: i64,
    latest: Arc<Mutex<Option<Direct3D11CaptureFrame>>>,
    /// Pictures the system sent, and pictures taken from them, for the log.
    arrived: Arc<AtomicU64>,
    taken: u64,
    closed: Arc<AtomicBool>,
    pool_size: SizeInt32,
    is_window: bool,
}

impl ScreenCapture {
    /// Starts capturing `target`, asking for at most one picture per `interval`: the system copies every picture it
    /// sends, and more than are written would be copied for nothing.
    pub fn start(graphics: &GraphicsDevice, target: CaptureTarget, interval: Duration) -> anyhow::Result<Self> {
        unsafe {
            let dxgi_device: IDXGIDevice = graphics.device.cast()?;
            let winrt_device: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)?.cast()?;

            let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
            let item: GraphicsCaptureItem = match target {
                CaptureTarget::Window(id) => interop
                    .CreateForWindow(HWND(id as usize as *mut _))
                    .context("cannot use the window as a capture target")?,
                CaptureTarget::Display(name) => interop
                    .CreateForMonitor(find_monitor(name))
                    .context("cannot use the display as a capture target")?,
            };
            let pool_size = item.Size()?;
            anyhow::ensure!(
                pool_size.Width > 0 && pool_size.Height > 0,
                "cannot get the size of the capture target"
            );

            let pool =
                Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt_device, PIXEL_FORMAT, POOL_BUFFERS, pool_size)?;

            let latest = Arc::new(Mutex::new(None::<Direct3D11CaptureFrame>));
            let handler_latest = latest.clone();
            let arrived = Arc::new(AtomicU64::new(0));
            let handler_arrived = arrived.clone();
            let frame_arrived = pool.FrameArrived(&TypedEventHandler::new(
                move |pool: Ref<Direct3D11CaptureFramePool>, _| {
                    if let Some(pool) = pool.as_ref() {
                        // Closing the old picture returns its buffer to the pool.
                        let previous = handler_latest.lock().unwrap().replace(pool.TryGetNextFrame()?);
                        handler_arrived.fetch_add(1, Ordering::Relaxed);
                        if let Some(previous) = previous {
                            let _ = previous.Close();
                        }
                    }
                    Ok(())
                },
            ))?;

            let closed = Arc::new(AtomicBool::new(false));
            let handler_closed = closed.clone();
            let item_closed = item.Closed(&TypedEventHandler::new(move |_, _| {
                handler_closed.store(true, Ordering::Relaxed);
                Ok(())
            }))?;

            let session = pool.CreateCaptureSession(&item)?;
            // Windows 11 24H2 and later; earlier versions send a picture for every change of the screen.
            let limit = TimeSpan {
                Duration: (interval.as_nanos() / 100) as i64,
            };
            if let Err(error) = session.SetMinUpdateInterval(limit) {
                log::info!("Capture cannot be limited to the frame rate: {error}");
            }
            session.StartCapture().context("cannot start capture")?;

            Ok(Self {
                winrt_device,
                item,
                pool,
                session,
                frame_arrived,
                item_closed,
                latest,
                arrived,
                taken: 0,
                closed,
                pool_size,
                is_window: matches!(target, CaptureTarget::Window(_)),
            })
        }
    }

    /// How many pictures the system has sent so far.
    #[cfg(test)]
    pub fn arrived(&self) -> u64 {
        self.arrived.load(Ordering::Relaxed)
    }

    /// If a new picture has arrived since the previous call, hands it to `handle` and returns what that returns.
    /// The picture goes back to the capture right after, so `handle` copies what it needs; the copy is queued on the
    /// same device before the capture can draw into that buffer again.
    ///
    /// After the window is closed, keeps returning `None` as if there were no new pictures
    /// (a window closing before the meeting ends is normal, so it is not treated as a failure).
    /// Errors if the display is gone.
    pub fn take_frame<T>(&mut self, handle: impl FnOnce(&Frame) -> anyhow::Result<T>) -> anyhow::Result<Option<T>> {
        if self.closed.load(Ordering::Relaxed) {
            anyhow::ensure!(self.is_window, "the display being captured is gone");
            return Ok(None);
        }
        let Some(frame) = self.latest.lock().unwrap().take() else {
            return Ok(None);
        };
        self.taken += 1;
        let handled = self.handle_frame(&frame, handle);
        // Return the buffer to the pool right away.
        let _ = frame.Close();
        handled.map(Some)
    }

    fn handle_frame<T>(
        &mut self,
        frame: &Direct3D11CaptureFrame,
        handle: impl FnOnce(&Frame) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        unsafe {
            let content = frame.ContentSize()?;
            // The first picture right after a size change is in a buffer of the old size.
            // Only the overlapping area is used.
            let width = content.Width.min(self.pool_size.Width).max(1) as u32;
            let height = content.Height.min(self.pool_size.Height).max(1) as u32;

            let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
            let texture: ID3D11Texture2D = access.GetInterface()?;
            let handled = handle(&Frame { texture, width, height })?;

            if content != self.pool_size && content.Width > 0 && content.Height > 0 {
                // From the next picture on, pictures arrive at the new size.
                self.pool
                    .Recreate(&self.winrt_device, PIXEL_FORMAT, POOL_BUFFERS, content)?;
                self.pool_size = content;
            }
            Ok(handled)
        }
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        log::info!(
            "capture: {} pictures arrived, {} taken",
            self.arrived.load(Ordering::Relaxed),
            self.taken
        );
        // If handlers or a held picture remain, the graphics device as a whole is never released.
        let _ = self.pool.RemoveFrameArrived(self.frame_arrived);
        let _ = self.item.RemoveClosed(self.item_closed);
        let _ = self.session.Close();
        if let Some(frame) = self.latest.lock().unwrap().take() {
            let _ = frame.Close();
        }
        let _ = self.pool.Close();
    }
}

fn find_monitor(display: Option<&str>) -> HMONITOR {
    struct Search<'a> {
        name: &'a str,
        found: Option<HMONITOR>,
    }

    unsafe extern "system" fn visit(monitor: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        if display_name(monitor).is_some_and(|name| name.eq_ignore_ascii_case(search.name)) {
            search.found = Some(monitor);
            return false.into();
        }
        true.into()
    }

    if let Some(name) = display {
        let mut search = Search { name, found: None };
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(visit), LPARAM(&raw mut search as isize));
        }
        if let Some(monitor) = search.found {
            return monitor;
        }
        log::warn!("Display {name} was not found; recording the main display.");
    }
    unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
}
