//! Turns captured pictures into the pictures the encoder takes: at the encode size, and in the encoder's format.
//!
//! The encode size is decided when recording starts and cannot change midway. Where a picture goes is decided by
//! [`placement`]: a picture of that size, or one pixel larger, is taken as it is or cropped; any other is scaled with
//! its aspect ratio kept and the margins filled with black.
//!
//! Pictures go to the encoder as NV12 (4:2:0, BT.709, limited range), which every H.264 encoder takes, from any GPU
//! vendor or Windows' software encoder. The Direct3D 11 video processor scales, crops and converts in one pass per
//! picture. A GPU whose video processor cannot write NV12 gets the captured BGRA instead, and the sink writer
//! converts it for the encoder.
//!
//! The pictures are drawn into a small pool of textures. A texture is drawn into again only once nothing holds the
//! picture in it: not the pipeline, which writes the same picture again until a new one arrives, and not the
//! encoder, which may read a picture after it was handed over. The views the video processor reads and writes
//! through are kept with the textures they belong to.

use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BOX, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_COLOR, D3D11_VIDEO_COLOR_0, D3D11_VIDEO_COLOR_RGBA,
    D3D11_VIDEO_COLOR_YCbCrA, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
    D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Texture2D, ID3D11VideoContext,
    ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView,
    ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::core::Interface;

use super::video::{CAPTURE_BUFFERS, Frame, GraphicsDevice};
use crate::recording::fit::{Placement, Rect, placement};

/// The pixel format of the pictures handed to the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureFormat {
    /// 4:2:0 YUV in BT.709 with the limited range of video.
    Nv12,
    /// 32-bit BGRA, as captured.
    Bgra,
}

/// The input color space of the video processor: full-range RGB.
const RGB_FULL_RANGE: D3D11_VIDEO_PROCESSOR_COLOR_SPACE = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 };
/// The output color space: BT.709 YCbCr (bit 2) with the 16-235 nominal range (bits 4-5 set to 1).
const YCBCR_BT709_LIMITED: D3D11_VIDEO_PROCESSOR_COLOR_SPACE = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
    _bitfield: 1 << 2 | 1 << 4,
};

/// A picture at the encode size, ready for the encoder.
///
/// Its texture is not drawn into again while this value lives or while a sample of it handed to the encoder does.
pub struct Picture {
    texture: ID3D11Texture2D,
    holder: Holder,
}

impl Picture {
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    /// Holds the texture for as long as the returned value lives, for a sample of it handed to the encoder.
    pub fn hold(&self) -> Holder {
        self.holder.clone()
    }
}

/// One holder of a texture of the pool. The texture can be drawn into again once every holder is dropped, on any
/// thread.
pub struct Holder(Arc<AtomicUsize>);

impl Holder {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self(count.clone())
    }
}

impl Clone for Holder {
    fn clone(&self) -> Self {
        Self::new(&self.0)
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        // Release: whoever finds the count at zero sees everything done with the texture before.
        self.0.fetch_sub(1, Ordering::Release);
    }
}

/// A texture at the encode size and how many hold the picture in it.
struct Pooled {
    texture: ID3D11Texture2D,
    holders: Arc<AtomicUsize>,
}

impl Pooled {
    fn new(texture: ID3D11Texture2D) -> Self {
        Self {
            texture,
            holders: Arc::default(),
        }
    }

    fn is_free(&self) -> bool {
        self.holders.load(Ordering::Acquire) == 0
    }

    fn picture(&self) -> Picture {
        Picture {
            texture: self.texture.clone(),
            holder: Holder::new(&self.holders),
        }
    }
}

pub struct Scaler {
    graphics: GraphicsDevice,
    width: u32,
    height: u32,
    format: PictureFormat,
    processor: Option<Processor>,
    /// Once the GPU scaler is known to be unusable, cropping is used instead (BGRA only).
    processor_unavailable: bool,
    /// The textures pictures are drawn into. It grows only while every texture is held.
    pool: Vec<Pooled>,
    /// The black picture, made once and never drawn into again.
    black: Option<Pooled>,
    /// Pictures drawn, for the log.
    drawn: u64,
}

/// The video processor configured for a given input size.
struct Processor {
    source: (u32, u32),
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    /// The views it writes the textures of the pool through, by their place in the pool.
    outputs: Vec<Option<ID3D11VideoProcessorOutputView>>,
    /// The views it reads the capture's buffers through. The capture hands over the same few buffers again and
    /// again; holding the view of one also holds the buffer, so no more are kept than the capture has.
    inputs: VecDeque<(ID3D11Texture2D, ID3D11VideoProcessorInputView)>,
}

impl Scaler {
    /// Pictures at `width` by `height`, in NV12 where the GPU's video processor can write it.
    pub fn new(graphics: &GraphicsDevice, width: u32, height: u32) -> Self {
        let graphics = GraphicsDevice {
            device: graphics.device.clone(),
            context: graphics.context.clone(),
        };
        let format = match writes_nv12(&graphics, (width, height)) {
            Ok(true) => PictureFormat::Nv12,
            Ok(false) => {
                log::info!("The GPU's video processor cannot write NV12; the encoder is given BGRA pictures.");
                PictureFormat::Bgra
            }
            Err(error) => {
                log::warn!("The GPU's video processor is not available; the encoder is given BGRA pictures: {error:#}");
                PictureFormat::Bgra
            }
        };
        Self {
            graphics,
            width,
            height,
            format,
            processor: None,
            processor_unavailable: false,
            pool: Vec::new(),
            black: None,
            drawn: 0,
        }
    }

    pub fn format(&self) -> PictureFormat {
        self.format
    }

    /// Draws `frame` at the encode size into a texture nothing holds, and returns it as a picture.
    pub fn fit(&mut self, frame: &Frame) -> anyhow::Result<Picture> {
        let canvas = (self.width, self.height);
        let place = placement((frame.width, frame.height), canvas);
        let slot = self.free_texture()?;
        let target = self.pool[slot].texture.clone();
        match self.format {
            PictureFormat::Nv12 => {
                // Taken as it is or cropped, the part of the picture that fits is drawn one to one. Every pixel of
                // the canvas is drawn: the processor fills what lies outside the destination with black.
                let (source, destination) = match place {
                    Placement::Whole | Placement::Crop => {
                        let whole = rect(0, 0, self.width, self.height);
                        (whole, whole)
                    }
                    Placement::Scale(fitted) => (rect(0, 0, frame.width, frame.height), to_rect(fitted)),
                };
                self.processor_for(frame)?
                    .blt(frame, source, slot, &target, destination)?;
            }
            PictureFormat::Bgra => self.fit_bgra(frame, place, slot, &target)?,
        }
        self.drawn += 1;
        Ok(self.pool[slot].picture())
    }

    fn fit_bgra(
        &mut self,
        frame: &Frame,
        place: Placement,
        slot: usize,
        target: &ID3D11Texture2D,
    ) -> anyhow::Result<()> {
        let fitted = match place {
            // The frame covers the whole canvas.
            Placement::Whole | Placement::Crop => {
                self.copy_top_left(frame, target);
                return Ok(());
            }
            Placement::Scale(fitted) => fitted,
        };

        self.clear(target)?;
        if !self.processor_unavailable {
            let source = rect(0, 0, frame.width, frame.height);
            match self
                .processor_for(frame)
                .and_then(|processor| processor.blt(frame, source, slot, target, to_rect(fitted)))
            {
                Ok(()) => return Ok(()),
                Err(error) => {
                    log::warn!("cannot scale video on the GPU; falling back to cropping: {error:#}");
                    self.processor_unavailable = true;
                }
            }
        }
        self.copy_top_left(frame, target);
        Ok(())
    }

    /// The place in the pool of a texture nothing holds, made when every texture is held.
    fn free_texture(&mut self) -> anyhow::Result<usize> {
        if let Some(slot) = self.pool.iter().position(Pooled::is_free) {
            return Ok(slot);
        }
        let texture = match self.format {
            PictureFormat::Nv12 => self.create_picture(None)?,
            PictureFormat::Bgra => self.graphics.create_texture(self.width, self.height)?,
        };
        self.pool.push(Pooled::new(texture));
        Ok(self.pool.len() - 1)
    }

    /// How many textures pictures are drawn into, and how many of them nothing holds.
    #[cfg(test)]
    pub fn textures(&self) -> (usize, usize) {
        (
            self.pool.len(),
            self.pool.iter().filter(|pooled| pooled.is_free()).count(),
        )
    }

    /// Aligns the top-left corners and cuts off whatever sticks out.
    fn copy_top_left(&self, frame: &Frame, target: &ID3D11Texture2D) {
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: frame.width.min(self.width),
            bottom: frame.height.min(self.height),
            back: 1,
        };
        unsafe {
            self.graphics
                .context
                .CopySubresourceRegion(target, 0, 0, 0, 0, &frame.texture, 0, Some(&region))
        };
    }

    /// A solid black picture at the encode size.
    pub fn black(&mut self) -> anyhow::Result<Picture> {
        if self.black.is_none() {
            self.black = Some(Pooled::new(self.draw_black()?));
        }
        Ok(self.black.as_ref().expect("made above").picture())
    }

    fn draw_black(&self) -> anyhow::Result<ID3D11Texture2D> {
        match self.format {
            PictureFormat::Nv12 => {
                // Black in limited range: luma 16, chroma 128. The chroma plane follows the luma plane.
                let (width, height) = (self.width as usize, self.height as usize);
                let mut planes = vec![16u8; width * height];
                planes.resize(width * height * 3 / 2, 128);
                let data = D3D11_SUBRESOURCE_DATA {
                    pSysMem: planes.as_ptr().cast(),
                    SysMemPitch: self.width,
                    SysMemSlicePitch: 0,
                };
                self.create_picture(Some(&data))
            }
            PictureFormat::Bgra => {
                let texture = self.graphics.create_texture(self.width, self.height)?;
                self.clear(&texture)?;
                Ok(texture)
            }
        }
    }

    /// An NV12 texture at the encode size that the video processor can write.
    fn create_picture(&self, initial: Option<&D3D11_SUBRESOURCE_DATA>) -> anyhow::Result<ID3D11Texture2D> {
        let description = D3D11_TEXTURE2D_DESC {
            Width: self.width,
            Height: self.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        unsafe {
            self.graphics
                .device
                .CreateTexture2D(&description, initial.map(std::ptr::from_ref), Some(&mut texture))
                .context("cannot create an NV12 picture")?
        };
        texture.context("cannot create the picture")
    }

    fn clear(&self, texture: &ID3D11Texture2D) -> anyhow::Result<()> {
        unsafe {
            let mut view = None;
            self.graphics
                .device
                .CreateRenderTargetView(texture, None, Some(&mut view))?;
            let view = view.context("cannot create the render target")?;
            self.graphics
                .context
                .ClearRenderTargetView(&view, &[0.0, 0.0, 0.0, 1.0]);
        }
        Ok(())
    }

    /// The video processor for pictures of `frame`'s size, created when the size changes.
    fn processor_for(&mut self, frame: &Frame) -> anyhow::Result<&mut Processor> {
        let source = (frame.width, frame.height);
        if self.processor.as_ref().is_none_or(|p| p.source != source) {
            self.processor = Some(Processor::create(
                &self.graphics,
                source,
                (self.width, self.height),
                self.format,
            )?);
        }
        Ok(self.processor.as_mut().expect("just created above"))
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        log::info!("pictures: {} drawn into {} textures", self.drawn, self.pool.len());
    }
}

/// Whether the video processor can take the captured BGRA and write NV12 at the encode size.
fn writes_nv12(graphics: &GraphicsDevice, canvas: (u32, u32)) -> anyhow::Result<bool> {
    unsafe {
        let video_device: ID3D11VideoDevice = graphics.device.cast()?;
        let enumerator = video_device.CreateVideoProcessorEnumerator(&content(canvas, canvas))?;
        let bgra = enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)?;
        let nv12 = enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_NV12)?;
        Ok(bgra & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 != 0
            && nv12 & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 != 0)
    }
}

fn content(source: (u32, u32), target: (u32, u32)) -> D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
    let rate = DXGI_RATIONAL {
        Numerator: 30,
        Denominator: 1,
    };
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
        InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        InputFrameRate: rate,
        InputWidth: source.0,
        InputHeight: source.1,
        OutputFrameRate: rate,
        OutputWidth: target.0,
        OutputHeight: target.1,
        Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    }
}

fn rect(left: u32, top: u32, right: u32, bottom: u32) -> RECT {
    RECT {
        left: left as i32,
        top: top as i32,
        right: right as i32,
        bottom: bottom as i32,
    }
}

fn to_rect(fitted: Rect) -> RECT {
    rect(fitted.left, fitted.top, fitted.right, fitted.bottom)
}

impl Processor {
    fn create(
        graphics: &GraphicsDevice,
        source: (u32, u32),
        target: (u32, u32),
        format: PictureFormat,
    ) -> anyhow::Result<Self> {
        unsafe {
            let video_device: ID3D11VideoDevice = graphics.device.cast()?;
            let video_context: ID3D11VideoContext = graphics.context.cast()?;
            let enumerator = video_device
                .CreateVideoProcessorEnumerator(&content(source, target))
                .context("cannot describe the video processor")?;
            let processor = video_device
                .CreateVideoProcessor(&enumerator, 0)
                .context("cannot create the video processor")?;
            if format == PictureFormat::Nv12 {
                video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &RGB_FULL_RANGE);
                video_context.VideoProcessorSetOutputColorSpace(&processor, &YCBCR_BT709_LIMITED);
            }
            // Outside the picture the output is black. For NV12 it is given in YCbCr: given in RGB, it was converted
            // to luma 0 rather than 16 on an NVIDIA GPU (2026-11-19), darker than the black of the picture itself.
            let (in_ycbcr, black) = match format {
                PictureFormat::Nv12 => (
                    true,
                    D3D11_VIDEO_COLOR_0 {
                        YCbCr: D3D11_VIDEO_COLOR_YCbCrA {
                            Y: 16.0 / 255.0,
                            Cb: 128.0 / 255.0,
                            Cr: 128.0 / 255.0,
                            A: 1.0,
                        },
                    },
                ),
                PictureFormat::Bgra => (
                    false,
                    D3D11_VIDEO_COLOR_0 {
                        RGBA: D3D11_VIDEO_COLOR_RGBA {
                            R: 0.0,
                            G: 0.0,
                            B: 0.0,
                            A: 1.0,
                        },
                    },
                ),
            };
            video_context.VideoProcessorSetOutputBackgroundColor(
                &processor,
                in_ycbcr,
                &D3D11_VIDEO_COLOR { Anonymous: black },
            );
            Ok(Self {
                source,
                video_device,
                video_context,
                enumerator,
                processor,
                outputs: Vec::new(),
                inputs: VecDeque::new(),
            })
        }
    }

    /// The view to write `target`, the texture at `slot` in the pool, through.
    fn output_view(&mut self, slot: usize, target: &ID3D11Texture2D) -> anyhow::Result<ID3D11VideoProcessorOutputView> {
        if let Some(Some(view)) = self.outputs.get(slot) {
            return Ok(view.clone());
        }
        let description = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut view = None;
        unsafe {
            self.video_device
                .CreateVideoProcessorOutputView(target, &self.enumerator, &description, Some(&mut view))
                .context("cannot write to the picture")?;
        }
        let view = view.context("cannot create the output view")?;
        if self.outputs.len() <= slot {
            self.outputs.resize(slot + 1, None);
        }
        self.outputs[slot] = Some(view.clone());
        Ok(view)
    }

    /// The view to read `texture`, a buffer of the capture, through.
    fn input_view(&mut self, texture: &ID3D11Texture2D) -> anyhow::Result<ID3D11VideoProcessorInputView> {
        if let Some((_, view)) = self.inputs.iter().find(|(kept, _)| kept.as_raw() == texture.as_raw()) {
            return Ok(view.clone());
        }
        let description = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut view = None;
        unsafe {
            self.video_device
                .CreateVideoProcessorInputView(texture, &self.enumerator, &description, Some(&mut view))
                .context("cannot read the captured picture")?;
        }
        let view = view.context("cannot create the input view")?;
        if self.inputs.len() == CAPTURE_BUFFERS {
            self.inputs.pop_front();
        }
        self.inputs.push_back((texture.clone(), view.clone()));
        Ok(view)
    }

    /// Draws the `source` part of `frame` into the `destination` part of `target`, the texture at `slot` in the pool.
    fn blt(
        &mut self,
        frame: &Frame,
        source: RECT,
        slot: usize,
        target: &ID3D11Texture2D,
        destination: RECT,
    ) -> anyhow::Result<()> {
        let output_view = self.output_view(slot, target)?;
        let input_view = self.input_view(&frame.texture)?;
        unsafe {
            self.video_context
                .VideoProcessorSetStreamSourceRect(&self.processor, 0, true, Some(&source));
            self.video_context
                .VideoProcessorSetStreamDestRect(&self.processor, 0, true, Some(&destination));

            let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(input_view)),
                ..Default::default()
            };
            let result =
                self.video_context
                    .VideoProcessorBlt(&self.processor, &output_view, 0, std::slice::from_ref(&stream));
            ManuallyDrop::drop(&mut stream.pInputSurface);
            result.context("the video processor failed")?;
            Ok(())
        }
    }
}
