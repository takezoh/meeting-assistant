//! Fits captured video to the size being encoded.
//!
//! The encode size is decided when recording starts and cannot change midway. Where a picture goes is decided by
//! [`placement`]: a picture of that size, or one pixel larger, is copied; any other is scaled with its aspect ratio
//! kept and the margins filled with black.

use std::mem::ManuallyDrop;

use anyhow::Context;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_VIDEO_COLOR, D3D11_VIDEO_COLOR_0, D3D11_VIDEO_COLOR_RGBA,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Texture2D, ID3D11VideoContext,
    ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_RATIONAL;
use windows::core::Interface;

use super::video::{Frame, GraphicsDevice};
use crate::recording::fit::{Placement, Rect, placement};

pub struct Scaler {
    graphics: GraphicsDevice,
    width: u32,
    height: u32,
    processor: Option<Processor>,
    /// Once the GPU scaler is known to be unusable, cropping is used instead.
    processor_unavailable: bool,
}

/// GPU scaling configured for a given input size.
struct Processor {
    source: (u32, u32),
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
}

impl Scaler {
    pub fn new(graphics: &GraphicsDevice, width: u32, height: u32) -> Self {
        Self {
            graphics: GraphicsDevice {
                device: graphics.device.clone(),
                context: graphics.context.clone(),
            },
            width,
            height,
            processor: None,
            processor_unavailable: false,
        }
    }

    /// Copies `frame` into a new texture at the encode size and returns it. The encoder may still read the texture
    /// after this returns, so it is never written again.
    pub fn fit(&mut self, frame: &Frame) -> anyhow::Result<ID3D11Texture2D> {
        let target = self.graphics.create_texture(self.width, self.height)?;
        let rect = match placement((frame.width, frame.height), (self.width, self.height)) {
            Placement::Whole | Placement::Crop => {
                self.copy_top_left(frame, &target);
                return Ok(target);
            }
            Placement::Scale(rect) => rect,
        };

        self.clear(&target)?;
        if !self.processor_unavailable {
            match self.scale(frame, &target, rect) {
                Ok(()) => return Ok(target),
                Err(error) => {
                    log::warn!("cannot scale video on the GPU; falling back to cropping: {error:#}");
                    self.processor_unavailable = true;
                }
            }
        }
        self.copy_top_left(frame, &target);
        Ok(target)
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

    /// A solid black texture at the encode size.
    pub fn black(&self) -> anyhow::Result<ID3D11Texture2D> {
        let texture = self.graphics.create_texture(self.width, self.height)?;
        self.clear(&texture)?;
        Ok(texture)
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

    fn scale(&mut self, frame: &Frame, target: &ID3D11Texture2D, rect: Rect) -> anyhow::Result<()> {
        let source = (frame.width, frame.height);
        if self.processor.as_ref().is_none_or(|p| p.source != source) {
            self.processor = Some(Processor::create(&self.graphics, source, (self.width, self.height))?);
        }
        let processor = self.processor.as_ref().expect("just created above");
        let destination = RECT {
            left: rect.left as i32,
            top: rect.top as i32,
            right: rect.right as i32,
            bottom: rect.bottom as i32,
        };
        processor.blt(frame, target, destination)
    }
}

impl Processor {
    fn create(graphics: &GraphicsDevice, source: (u32, u32), target: (u32, u32)) -> anyhow::Result<Self> {
        unsafe {
            let video_device: ID3D11VideoDevice = graphics.device.cast()?;
            let video_context: ID3D11VideoContext = graphics.context.cast()?;
            let rate = DXGI_RATIONAL {
                Numerator: 30,
                Denominator: 1,
            };
            let description = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: source.0,
                InputHeight: source.1,
                OutputFrameRate: rate,
                OutputWidth: target.0,
                OutputHeight: target.1,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            };
            let enumerator = video_device.CreateVideoProcessorEnumerator(&description)?;
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
            Ok(Self {
                source,
                video_device,
                video_context,
                enumerator,
                processor,
            })
        }
    }

    fn blt(&self, frame: &Frame, target: &ID3D11Texture2D, destination: RECT) -> anyhow::Result<()> {
        unsafe {
            let output_description = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view = None;
            self.video_device.CreateVideoProcessorOutputView(
                target,
                &self.enumerator,
                &output_description,
                Some(&mut output_view),
            )?;
            let output_view = output_view.context("cannot create the output view")?;

            let input_description = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: 0,
                    },
                },
            };
            let mut input_view = None;
            self.video_device.CreateVideoProcessorInputView(
                &frame.texture,
                &self.enumerator,
                &input_description,
                Some(&mut input_view),
            )?;
            let input_view = input_view.context("cannot create the input view")?;

            let source = RECT {
                left: 0,
                top: 0,
                right: frame.width as i32,
                bottom: frame.height as i32,
            };
            self.video_context
                .VideoProcessorSetStreamSourceRect(&self.processor, 0, true, Some(&source));
            self.video_context
                .VideoProcessorSetStreamDestRect(&self.processor, 0, true, Some(&destination));
            let black = D3D11_VIDEO_COLOR {
                Anonymous: D3D11_VIDEO_COLOR_0 {
                    RGBA: D3D11_VIDEO_COLOR_RGBA {
                        R: 0.0,
                        G: 0.0,
                        B: 0.0,
                        A: 1.0,
                    },
                },
            };
            self.video_context
                .VideoProcessorSetOutputBackgroundColor(&self.processor, false, &black);

            let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(input_view)),
                ..Default::default()
            };
            let result =
                self.video_context
                    .VideoProcessorBlt(&self.processor, &output_view, 0, std::slice::from_ref(&stream));
            ManuallyDrop::drop(&mut stream.pInputSurface);
            result?;
            Ok(())
        }
    }
}
