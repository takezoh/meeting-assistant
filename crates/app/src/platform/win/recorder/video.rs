//! Captures one window or one display with Windows.Graphics.Capture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
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
const POOL_BUFFERS: i32 = 3;

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

/// One captured picture. The texture is not modified afterwards.
pub struct Frame {
    pub texture: ID3D11Texture2D,
    pub width: u32,
    pub height: u32,
}

/// The capture. Keeps only the latest of the arriving pictures.
pub struct ScreenCapture {
    graphics_device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_arrived: i64,
    item_closed: i64,
    latest: Arc<Mutex<Option<Direct3D11CaptureFrame>>>,
    closed: Arc<AtomicBool>,
    pool_size: SizeInt32,
    is_window: bool,
}

impl ScreenCapture {
    pub fn start(graphics: &GraphicsDevice, target: CaptureTarget) -> anyhow::Result<Self> {
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
            let frame_arrived = pool.FrameArrived(&TypedEventHandler::new(
                move |pool: Ref<Direct3D11CaptureFramePool>, _| {
                    if let Some(pool) = pool.as_ref() {
                        // Closing the old picture returns its buffer to the pool.
                        let previous = handler_latest.lock().unwrap().replace(pool.TryGetNextFrame()?);
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
            session.StartCapture().context("cannot start capture")?;

            Ok(Self {
                graphics_device: graphics.device.clone(),
                context: graphics.context.clone(),
                winrt_device,
                item,
                pool,
                session,
                frame_arrived,
                item_closed,
                latest,
                closed,
                pool_size,
                is_window: matches!(target, CaptureTarget::Window(_)),
            })
        }
    }

    /// If a new picture has arrived since the previous call, duplicates it into a dedicated texture and returns it.
    ///
    /// After the window is closed, keeps returning `None` as if there were no new pictures
    /// (a window closing before the meeting ends is normal, so it is not treated as a failure).
    /// Errors if the display is gone.
    pub fn take_frame(&mut self) -> anyhow::Result<Option<Frame>> {
        if self.closed.load(Ordering::Relaxed) {
            anyhow::ensure!(self.is_window, "the display being captured is gone");
            return Ok(None);
        }
        let Some(frame) = self.latest.lock().unwrap().take() else {
            return Ok(None);
        };
        let copied = self.copy_frame(&frame);
        // Return the buffer to the pool right away.
        let _ = frame.Close();
        copied.map(Some)
    }

    fn copy_frame(&mut self, frame: &Direct3D11CaptureFrame) -> anyhow::Result<Frame> {
        unsafe {
            let content = frame.ContentSize()?;
            // The first picture right after a size change is in a buffer of the old size.
            // Only the overlapping area is used.
            let width = content.Width.min(self.pool_size.Width).max(1) as u32;
            let height = content.Height.min(self.pool_size.Height).max(1) as u32;

            let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
            let source: ID3D11Texture2D = access.GetInterface()?;
            let texture = GraphicsDevice {
                device: self.graphics_device.clone(),
                context: self.context.clone(),
            }
            .create_texture(width, height)?;
            let region = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: width,
                bottom: height,
                back: 1,
            };
            self.context
                .CopySubresourceRegion(&texture, 0, 0, 0, 0, &source, 0, Some(&region));

            if content != self.pool_size && content.Width > 0 && content.Height > 0 {
                // From the next picture on, pictures arrive at the new size.
                self.pool
                    .Recreate(&self.winrt_device, PIXEL_FORMAT, POOL_BUFFERS, content)?;
                self.pool_size = content;
            }
            Ok(Frame { texture, width, height })
        }
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
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
