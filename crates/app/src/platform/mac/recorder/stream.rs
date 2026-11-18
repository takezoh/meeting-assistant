//! One ScreenCaptureKit stream: the pictures of a window or a display, or the sound of an app or of everything.
//!
//! ScreenCaptureKit delivers samples on its own queue. Pictures are kept as the latest one only; sound goes into a
//! [`SourceBuffer`] in the mixer's format. The recording thread takes them when it needs them.

use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMBlockBuffer, CMSampleBuffer, CMTime,
    kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
};
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};

use crate::recording::mixer::{BufferPolicy, SourceBuffer};
use crate::recording::pcm;
use crate::recording::plan::{CHANNELS, SAMPLE_RATE};

const START_TIMEOUT: Duration = Duration::from_secs(10);
/// '420v': two-plane 4:2:0 in video range, which the H.264 encoder takes without converting.
pub const PIXEL_FORMAT: u32 = u32::from_be_bytes(*b"420v");
/// Pictures ScreenCaptureKit may hold out at once before it waits for them to be released.
const QUEUE_DEPTH: isize = 6;

/// A picture: a frame ScreenCaptureKit captured, or one this app made. Shared between threads read-only.
#[derive(Clone)]
pub struct Picture(pub CFRetained<CVPixelBuffer>);
// SAFETY: pixel buffers are reference-counted Core Foundation objects that are safe to retain and read from any
// thread; nothing writes to a captured frame.
unsafe impl Send for Picture {}
// SAFETY: as above.
unsafe impl Sync for Picture {}

/// What the stream's queue hands over to the recording thread.
pub struct Received {
    latest: Mutex<Option<Picture>>,
    sound: Mutex<SourceBuffer>,
    stopped: Mutex<Option<String>>,
}

impl Received {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            latest: Mutex::new(None),
            sound: Mutex::new(SourceBuffer::new(BufferPolicy::for_format(SAMPLE_RATE, CHANNELS))),
            stopped: Mutex::new(None),
        })
    }

    fn picture(&self, sample: &CMSampleBuffer) {
        // SAFETY: reading the sample's image, if it has one (frames without changes have none).
        if let Some(image) = unsafe { sample.image_buffer() } {
            *self.latest.lock().unwrap() = Some(Picture(image));
        }
    }

    fn sound(&self, sample: &CMSampleBuffer) {
        if let Some(pcm) = with_float_channels(sample, |channels| pcm::interleave_f32_to_i16(channels, CHANNELS)) {
            self.sound.lock().unwrap().push(&pcm);
        }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "MeetingAssistantStreamOutput"]
    #[ivars = Arc<Received>]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            match kind {
                SCStreamOutputType::Screen => self.ivars().picture(sample),
                SCStreamOutputType::Audio => self.ivars().sound(sample),
                _ => {}
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamOutput {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            *self.ivars().stopped.lock().unwrap() = Some(error.to_string());
        }
    }
);

impl StreamOutput {
    fn new(received: Arc<Received>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(received);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// What a stream delivers.
pub enum Kind {
    /// Pictures at `size` pixels, the picture fitted into it with its aspect ratio kept.
    Pictures { size: (u32, u32), framerate: u32 },
    /// The sound of what the filter includes, in the mixer's format.
    Sound,
}

/// A running capture.
pub struct Capture {
    stream: Retained<SCStream>,
    _output: Retained<StreamOutput>,
    _queue: DispatchRetained<DispatchQueue>,
    received: Arc<Received>,
}

impl Capture {
    pub fn start(filter: &SCContentFilter, kind: &Kind) -> anyhow::Result<Self> {
        let received = Received::new();
        let output = StreamOutput::new(received.clone());
        let queue = DispatchQueue::new("meeting-assistant.capture", None);
        // SAFETY: configuring a fresh configuration object.
        let (config, output_type) = unsafe {
            let config = SCStreamConfiguration::new();
            config.setQueueDepth(QUEUE_DEPTH);
            match kind {
                Kind::Pictures { size, framerate } => {
                    config.setWidth(size.0 as usize);
                    config.setHeight(size.1 as usize);
                    config.setPixelFormat(PIXEL_FORMAT);
                    config.setMinimumFrameInterval(CMTime::new(1, (*framerate).max(1) as i32));
                    config.setScalesToFit(true);
                    config.setPreservesAspectRatio(true);
                    config.setShowsCursor(true);
                    (config, SCStreamOutputType::Screen)
                }
                Kind::Sound => {
                    // The pictures of a sound stream are not used; keep them tiny and rare.
                    config.setWidth(2);
                    config.setHeight(2);
                    config.setMinimumFrameInterval(CMTime::new(1, 1));
                    config.setCapturesAudio(true);
                    config.setSampleRate(SAMPLE_RATE as isize);
                    config.setChannelCount(CHANNELS as isize);
                    config.setExcludesCurrentProcessAudio(true);
                    (config, SCStreamOutputType::Audio)
                }
            }
        };
        // SAFETY: the filter, the configuration and the delegate are valid; the output and the queue are kept for the
        // life of the stream.
        let stream = unsafe {
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                filter,
                &config,
                Some(ProtocolObject::from_ref(&*output)),
            );
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    output_type,
                    Some(&queue),
                )
                .map_err(|error| anyhow::anyhow!("cannot receive from the capture: {error}"))?;
            stream
        };
        wait_for(|handler| unsafe { stream.startCaptureWithCompletionHandler(Some(handler)) })
            .context("cannot start capturing")?;
        Ok(Self {
            stream,
            _output: output,
            _queue: queue,
            received,
        })
    }

    /// The picture captured since the previous call, if any.
    pub fn take_picture(&self) -> Option<Picture> {
        self.received.latest.lock().unwrap().take()
    }

    /// Why the capture stopped by itself (the window closed, the display went away), if it did.
    pub fn stopped(&self) -> Option<String> {
        self.received.stopped.lock().unwrap().clone()
    }

    pub fn mix_into(&self, out: &mut [i16]) {
        self.received.sound.lock().unwrap().mix_into(out);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: stopping a stream this capture started.
        let _ = wait_for(|handler| unsafe { self.stream.stopCaptureWithCompletionHandler(Some(handler)) });
    }
}

/// Calls a ScreenCaptureKit function that reports completion through a handler, and waits for it.
fn wait_for(call: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>)) -> anyhow::Result<()> {
    let (sender, receiver) = mpsc::channel::<Option<String>>();
    let handler = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: ScreenCaptureKit passes a valid error or null.
        let error = unsafe { Retained::retain(error) }.map(|error| error.to_string());
        let _ = sender.send(error);
    });
    call(&handler);
    match receiver.recv_timeout(START_TIMEOUT) {
        Ok(None) => Ok(()),
        Ok(Some(error)) => anyhow::bail!(error),
        Err(_) => anyhow::bail!("ScreenCaptureKit did not answer in time"),
    }
}

/// Hands the float samples of an audio sample buffer, one slice per channel, to `convert`, without copying them when
/// they are already one buffer per channel. `None` when the buffer is not 32-bit float audio.
fn with_float_channels<R>(sample: &CMSampleBuffer, convert: impl FnOnce(&[&[f32]]) -> R) -> Option<R> {
    // SAFETY: reading the sample's format; an audio sample's description is an audio format description.
    let format = unsafe {
        let description = sample.format_description()?;
        CMAudioFormatDescriptionGetStreamBasicDescription(&description).as_ref()?
    };
    if format.mFormatFlags & kAudioFormatFlagIsFloat == 0 || format.mBitsPerChannel != 32 {
        return None;
    }
    let channels = format.mChannelsPerFrame.max(1) as usize;
    let planar = format.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0;

    /// Room for as many buffers as there can be channels here.
    #[repr(C)]
    struct List {
        count: u32,
        buffers: [AudioBuffer; 8],
    }
    // SAFETY: plain data; zeroed is valid.
    let mut list: List = unsafe { std::mem::zeroed() };
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: `list` has room for 8 buffers, more than the two channels configured.
    let status = unsafe {
        sample.audio_buffer_list_with_retained_block_buffer(
            std::ptr::null_mut(),
            (&raw mut list).cast::<AudioBufferList>(),
            size_of::<List>(),
            None,
            None,
            kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
            &mut block,
        )
    };
    if status != 0 || block.is_null() {
        return None;
    }
    // SAFETY: a +1 reference that keeps the buffers' memory alive while they are read.
    let _block = unsafe { CFRetained::from_raw(NonNull::new_unchecked(block)) };
    let buffers = &list.buffers[..(list.count as usize).min(list.buffers.len())];
    let slice = |buffer: &AudioBuffer| -> &[f32] {
        if buffer.mData.is_null() {
            return &[];
        }
        // SAFETY: the buffer holds `mDataByteSize` bytes of 32-bit floats while the block lives.
        unsafe { std::slice::from_raw_parts(buffer.mData.cast::<f32>(), buffer.mDataByteSize as usize / 4) }
    };
    if planar {
        let channels: Vec<&[f32]> = buffers.iter().map(slice).collect();
        Some(convert(&channels))
    } else {
        let split = pcm::deinterleave(slice(buffers.first()?), channels);
        let channels: Vec<&[f32]> = split.iter().map(Vec::as_slice).collect();
        Some(convert(&channels))
    }
}
