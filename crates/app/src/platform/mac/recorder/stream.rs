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
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, kAudioFormatFlagIsFloat,
    kAudioFormatFlagIsNonInterleaved,
};
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
use crate::recording::pcm::{self, SoundReport};
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
    report: Mutex<SoundReport>,
    stopped: Mutex<Option<String>>,
}

impl Received {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            latest: Mutex::new(None),
            sound: Mutex::new(SourceBuffer::new(BufferPolicy::for_format(SAMPLE_RATE, CHANNELS))),
            report: Mutex::new(SoundReport::default()),
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
        let mut report = self.report.lock().unwrap();
        if let Some(format) = stream_format(sample) {
            report.format(|| describe(&format));
        }
        let converted = with_float_channels(sample, |channels| {
            report.usable(channels);
            pcm::interleave_f32_to_i16(channels, CHANNELS)
        });
        match converted {
            Ok(pcm) => {
                drop(report);
                self.sound.lock().unwrap().push(&pcm);
            }
            Err(problem) => report.unusable(problem),
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
    /// Whether this capture delivers sound, whose report is logged when it ends.
    is_sound: bool,
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
            is_sound: matches!(kind, Kind::Sound),
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
        if self.is_sound {
            let report = self.received.report.lock().unwrap();
            match self.stopped() {
                Some(reason) => log::info!("Sound capture ended: {report}; it had stopped by itself: {reason}"),
                None => log::info!("Sound capture ended: {report}"),
            }
        }
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

/// The stream description of an audio sample, if it has one.
fn stream_format(sample: &CMSampleBuffer) -> Option<AudioStreamBasicDescription> {
    // SAFETY: reading the sample's format; an audio sample's description is an audio format description. The
    // description is copied out while the format description is alive.
    unsafe {
        let description = sample.format_description()?;
        CMAudioFormatDescriptionGetStreamBasicDescription(&description)
            .as_ref()
            .copied()
    }
}

fn describe(format: &AudioStreamBasicDescription) -> String {
    format!(
        "{} Hz, {} channels, {} bit, flags {:#x}",
        format.mSampleRate, format.mChannelsPerFrame, format.mBitsPerChannel, format.mFormatFlags
    )
}

/// Hands the float samples of an audio sample buffer, one slice per channel, to `convert`, without copying them when
/// they are already one buffer per channel. Fails, saying why, when the buffer is not 32-bit float audio or its
/// samples cannot be read.
fn with_float_channels<R>(sample: &CMSampleBuffer, convert: impl FnOnce(&[&[f32]]) -> R) -> Result<R, String> {
    let format = stream_format(sample).ok_or("the sample has no audio format")?;
    if format.mFormatFlags & kAudioFormatFlagIsFloat == 0 || format.mBitsPerChannel != 32 {
        return Err(format!("not 32-bit float ({})", describe(&format)));
    }
    let channels = format.mChannelsPerFrame.max(1) as usize;
    let planar = format.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0;

    // Core Media fills the buffer list only when it is given exactly the size it needs; a larger list fails with
    // kCMSampleBufferError_ArrayTooSmall. Ask for the size first.
    let mut needed: usize = 0;
    // SAFETY: only the needed size is written.
    let status = unsafe {
        sample.audio_buffer_list_with_retained_block_buffer(
            &mut needed,
            std::ptr::null_mut(),
            0,
            None,
            None,
            0,
            std::ptr::null_mut(),
        )
    };
    let header = std::mem::offset_of!(AudioBufferList, mBuffers);
    if status != 0 || needed < header {
        return Err(format!("cannot size its buffer list ({status}, {needed} bytes)"));
    }
    // Zeroed storage of `needed` bytes, aligned for the list's pointers.
    let mut storage = vec![0u64; needed.div_ceil(size_of::<u64>())];
    let list = storage.as_mut_ptr().cast::<AudioBufferList>();
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: `list` points to `needed` writable, aligned bytes.
    let status = unsafe {
        sample.audio_buffer_list_with_retained_block_buffer(
            std::ptr::null_mut(),
            list,
            needed,
            None,
            None,
            kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
            &mut block,
        )
    };
    if status != 0 || block.is_null() {
        return Err(format!("cannot read its samples ({status})"));
    }
    // SAFETY: a +1 reference that keeps the buffers' memory alive while they are read.
    let _block = unsafe { CFRetained::from_raw(NonNull::new_unchecked(block)) };
    // SAFETY: Core Media wrote the list into `storage`; the count is checked against the bytes it occupies.
    let buffers = unsafe {
        let count = (*list).mNumberBuffers as usize;
        if header + count * size_of::<AudioBuffer>() > needed {
            return Err(format!(
                "its buffer list does not fit ({count} buffers in {needed} bytes)"
            ));
        }
        std::slice::from_raw_parts((&raw const (*list).mBuffers).cast::<AudioBuffer>(), count)
    };
    let slice = |buffer: &AudioBuffer| -> &[f32] {
        if buffer.mData.is_null() {
            return &[];
        }
        // SAFETY: the buffer holds `mDataByteSize` bytes of 32-bit floats while the block lives.
        unsafe { std::slice::from_raw_parts(buffer.mData.cast::<f32>(), buffer.mDataByteSize as usize / 4) }
    };
    if planar {
        let channels: Vec<&[f32]> = buffers.iter().map(slice).collect();
        Ok(convert(&channels))
    } else {
        let split = pcm::deinterleave(slice(buffers.first().ok_or("the sample has no buffer")?), channels);
        let channels: Vec<&[f32]> = split.iter().map(Vec::as_slice).collect();
        Ok(convert(&channels))
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_audio_types::{kAudioFormatFlagIsPacked, kAudioFormatLinearPCM};
    use objc2_core_media::{
        CMAudioFormatDescription, CMAudioFormatDescriptionCreate, CMAudioSampleBufferCreateReadyWithPacketDescriptions,
        CMFormatDescription, kCMBlockBufferAssureMemoryNowFlag,
    };

    use super::*;

    /// A sample of 32-bit float audio. `planar` is the format ScreenCaptureKit delivers sound in: one buffer per
    /// channel; otherwise the channels are interleaved in one buffer.
    fn float_sample(channels: &[&[f32]], planar: bool) -> CFRetained<CMSampleBuffer> {
        let (layout, bytes_per_frame) = if planar {
            (kAudioFormatFlagIsNonInterleaved, 4)
        } else {
            (0, 4 * channels.len() as u32)
        };
        let description = AudioStreamBasicDescription {
            mSampleRate: f64::from(SAMPLE_RATE),
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked | layout,
            mBytesPerPacket: bytes_per_frame,
            mFramesPerPacket: 1,
            mBytesPerFrame: bytes_per_frame,
            mChannelsPerFrame: channels.len() as u32,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        let mut format: *const CMAudioFormatDescription = std::ptr::null();
        // SAFETY: the description and the output are valid for the call.
        let status = unsafe {
            CMAudioFormatDescriptionCreate(
                None,
                NonNull::from(&description),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                None,
                NonNull::from(&mut format),
            )
        };
        assert_eq!(status, 0);
        // SAFETY: a +1 reference from the call above.
        let format = unsafe { CFRetained::from_raw(NonNull::new(format.cast_mut()).unwrap()) };

        // A planar block holds one channel's samples after the other.
        let samples: Vec<f32> = if planar {
            channels.concat()
        } else {
            (0..channels[0].len())
                .flat_map(|frame| channels.iter().map(move |channel| channel[frame]))
                .collect()
        };
        let bytes = size_of_val(samples.as_slice());
        let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
        // SAFETY: Core Media allocates `bytes` of memory for the block, then the samples are copied into it.
        let block = unsafe {
            let status = CMBlockBuffer::create_with_memory_block(
                None,
                std::ptr::null_mut(),
                bytes,
                None,
                std::ptr::null(),
                0,
                bytes,
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            );
            assert_eq!(status, 0);
            let block = CFRetained::from_raw(NonNull::new(block).unwrap());
            assert_eq!(
                CMBlockBuffer::replace_data_bytes(NonNull::from(samples.as_slice()).cast(), &block, 0, bytes),
                0
            );
            block
        };

        let mut sample: *mut CMSampleBuffer = std::ptr::null_mut();
        // SAFETY: the block and the format describe the same data.
        let status = unsafe {
            CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &block,
                AsRef::<CMFormatDescription>::as_ref(&*format),
                channels[0].len() as isize,
                CMTime::new(0, SAMPLE_RATE as i32),
                std::ptr::null(),
                NonNull::from(&mut sample),
            )
        };
        assert_eq!(status, 0);
        // SAFETY: a +1 reference from the call above.
        unsafe { CFRetained::from_raw(NonNull::new(sample).unwrap()) }
    }

    #[test]
    fn a_planar_float_sample_is_read_one_slice_per_channel() {
        let sample = float_sample(&[&[0.5, -0.5, 0.25], &[0.125, 0.0, -1.0]], true);
        assert_eq!(read(&sample), Ok(vec![vec![0.5, -0.5, 0.25], vec![0.125, 0.0, -1.0]]));
    }

    #[test]
    fn an_interleaved_float_sample_is_split_into_channels() {
        let sample = float_sample(&[&[0.5, -0.5], &[0.125, 1.0]], false);
        assert_eq!(read(&sample), Ok(vec![vec![0.5, -0.5], vec![0.125, 1.0]]));
    }

    fn read(sample: &CMSampleBuffer) -> Result<Vec<Vec<f32>>, String> {
        with_float_channels(sample, |channels| {
            channels.iter().map(|channel| channel.to_vec()).collect()
        })
    }
}
