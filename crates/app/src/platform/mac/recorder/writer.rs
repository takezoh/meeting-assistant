//! Writing a recording on macOS: H.264 and AAC into an MP4 with AVAssetWriter, encoded by the system's encoders
//! (hardware where the Mac has it). Video is encoded at the configured quality rather than a fixed bitrate, so a
//! still screen makes a small file. Movie fragments are written every few seconds, so the file stays playable up to
//! the last fragment if the process dies.

use std::path::Path;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use block2::RcBlock;
use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4,
    AVMediaTypeAudio, AVMediaTypeVideo, AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey,
    AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey,
    AVVideoProfileLevelH264MainAutoLevel, AVVideoProfileLevelKey, AVVideoQualityKey, AVVideoWidthKey,
};
use objc2_avf_audio::{AVEncoderBitRateKey, AVFormatIDKey, AVNumberOfChannelsKey, AVSampleRateKey};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsPacked, kAudioFormatFlagIsSignedInteger, kAudioFormatLinearPCM,
    kAudioFormatMPEG4AAC,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescription, CMAudioFormatDescriptionCreate, CMAudioSampleBufferCreateReadyWithPacketDescriptions,
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime, kCMBlockBufferAssureMemoryNowFlag,
};
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::recording::plan::{CHANNELS, MediaFormat, SAMPLE_RATE};

/// The pipeline's times are in 100 ns units.
const TIMESCALE: i32 = 10_000_000;
/// How often a movie fragment is written: at most this much is lost if the process dies.
const FRAGMENT_INTERVAL_SECONDS: i64 = 2;
const AUDIO_BITRATE: u32 = 128_000;
const KEYFRAME_INTERVAL_SECONDS: u32 = 2;
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Mp4Writer {
    writer: Retained<AVAssetWriter>,
    video: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    audio: Option<AudioInput>,
}

struct AudioInput {
    input: Retained<AVAssetWriterInput>,
    format: CFRetained<CMAudioFormatDescription>,
}

impl Mp4Writer {
    pub fn create(path: &Path, format: &MediaFormat) -> anyhow::Result<Self> {
        // AVAssetWriter does not replace an existing file.
        let _ = std::fs::remove_file(path);
        let url = NSURL::from_file_path(path).context("the output path is not a file URL")?;
        // SAFETY: the constants are exported by AVFoundation.
        let file_type = unsafe { AVFileTypeMPEG4 }.context("AVFoundation does not name the MP4 type")?;
        // SAFETY: plain AVFoundation calls with valid arguments; all objects are used on this thread.
        unsafe {
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|error| anyhow::anyhow!("cannot create the output file: {error}"))?;
            writer.setMovieFragmentInterval(seconds(FRAGMENT_INTERVAL_SECONDS));

            let video = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                AVMediaTypeVideo.context("no video media type")?,
                Some(&*video_settings(format)?),
            );
            video.setExpectsMediaDataInRealTime(true);
            anyhow::ensure!(
                writer.canAddInput(&video),
                "the writer does not take the video settings"
            );
            writer.addInput(&video);
            let adaptor =
                AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                    &video, None,
                );

            let audio = if format.has_audio {
                let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                    AVMediaTypeAudio.context("no audio media type")?,
                    Some(&*audio_settings()?),
                );
                input.setExpectsMediaDataInRealTime(true);
                anyhow::ensure!(
                    writer.canAddInput(&input),
                    "the writer does not take the audio settings"
                );
                writer.addInput(&input);
                Some(AudioInput {
                    input,
                    format: pcm_format()?,
                })
            } else {
                None
            };

            anyhow::ensure!(
                writer.startWriting(),
                "cannot start writing: {}",
                describe_error(&writer)
            );
            writer.startSessionAtSourceTime(time(0));
            Ok(Self {
                writer,
                video,
                adaptor,
                audio,
            })
        }
    }

    /// `time` is in 100 ns units. A picture the encoder cannot take yet is dropped; the previous one stays on screen.
    pub fn write_video(&self, picture: &CVPixelBuffer, time_100ns: i64) -> anyhow::Result<()> {
        self.ensure_writing()?;
        // SAFETY: the picture is a valid pixel buffer; the inputs belong to this writer.
        unsafe {
            if self.video.isReadyForMoreMediaData() {
                self.adaptor
                    .appendPixelBuffer_withPresentationTime(picture, time(time_100ns));
            }
        }
        Ok(())
    }

    /// `pcm` is interleaved 16-bit stereo at the pipeline's rate; `time` is in 100 ns units.
    pub fn write_audio(&self, pcm: &[i16], time_100ns: i64) -> anyhow::Result<()> {
        let Some(audio) = &self.audio else { return Ok(()) };
        if pcm.is_empty() {
            return Ok(());
        }
        self.ensure_writing()?;
        let sample = pcm_sample(pcm, &audio.format, time(time_100ns))?;
        // SAFETY: the sample matches the input's source format.
        unsafe {
            if audio.input.isReadyForMoreMediaData() {
                audio.input.appendSampleBuffer(&sample);
            }
        }
        Ok(())
    }

    /// Writes out the rest and closes the file.
    pub fn finalize(self) -> anyhow::Result<()> {
        let (done, finished) = mpsc::channel();
        // SAFETY: the inputs and the writer belong together; the handler only signals.
        unsafe {
            self.video.markAsFinished();
            if let Some(audio) = &self.audio {
                audio.input.markAsFinished();
            }
            let handler = RcBlock::new(move || {
                let _ = done.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&handler);
        }
        finished
            .recv_timeout(FINALIZE_TIMEOUT)
            .context("finishing the file did not complete")?;
        // SAFETY: reading the writer's status.
        let status = unsafe { self.writer.status() };
        anyhow::ensure!(
            status == AVAssetWriterStatus::Completed,
            "the file could not be finished: {}",
            describe_error(&self.writer)
        );
        Ok(())
    }

    fn ensure_writing(&self) -> anyhow::Result<()> {
        // SAFETY: reading the writer's status.
        let status = unsafe { self.writer.status() };
        anyhow::ensure!(
            status == AVAssetWriterStatus::Writing,
            "writing stopped: {}",
            describe_error(&self.writer)
        );
        Ok(())
    }
}

fn describe_error(writer: &AVAssetWriter) -> String {
    // SAFETY: reading the writer's error.
    unsafe { writer.error() }.map_or_else(|| "unknown error".to_owned(), |error| error.to_string())
}

fn time(value_100ns: i64) -> CMTime {
    // SAFETY: plain constructor.
    unsafe { CMTime::new(value_100ns, TIMESCALE) }
}

fn seconds(value: i64) -> CMTime {
    time(value * i64::from(TIMESCALE))
}

fn object<T: Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an `AnyObject`.
    unsafe { Retained::cast_unchecked(value) }
}

fn key(key: Option<&'static NSString>) -> anyhow::Result<&'static NSString> {
    key.context("AVFoundation does not name a settings key")
}

fn video_settings(format: &MediaFormat) -> anyhow::Result<Retained<NSDictionary<NSString, AnyObject>>> {
    // SAFETY: the keys and values are constants exported by AVFoundation.
    unsafe {
        let compression = NSDictionary::<NSString, AnyObject>::from_retained_objects(
            &[
                key(AVVideoQualityKey)?,
                key(AVVideoExpectedSourceFrameRateKey)?,
                key(AVVideoMaxKeyFrameIntervalKey)?,
                key(AVVideoProfileLevelKey)?,
            ],
            &[
                object(NSNumber::new_f64(f64::from(format.quality) / 100.0)),
                object(NSNumber::new_u32(format.framerate)),
                object(NSNumber::new_u32(format.framerate * KEYFRAME_INTERVAL_SECONDS)),
                object(key(AVVideoProfileLevelH264MainAutoLevel)?.retain()),
            ],
        );
        Ok(NSDictionary::from_retained_objects(
            &[
                key(AVVideoCodecKey)?,
                key(AVVideoWidthKey)?,
                key(AVVideoHeightKey)?,
                key(AVVideoCompressionPropertiesKey)?,
            ],
            &[
                object(AVVideoCodecTypeH264.context("no H.264 codec type")?.retain()),
                object(NSNumber::new_u32(format.width)),
                object(NSNumber::new_u32(format.height)),
                object(compression),
            ],
        ))
    }
}

fn audio_settings() -> anyhow::Result<Retained<NSDictionary<NSString, AnyObject>>> {
    // SAFETY: the keys are constants exported by AVFAudio.
    unsafe {
        Ok(NSDictionary::from_retained_objects(
            &[
                key(AVFormatIDKey)?,
                key(AVSampleRateKey)?,
                key(AVNumberOfChannelsKey)?,
                key(AVEncoderBitRateKey)?,
            ],
            &[
                object(NSNumber::new_u32(kAudioFormatMPEG4AAC)),
                object(NSNumber::new_u32(SAMPLE_RATE)),
                object(NSNumber::new_usize(CHANNELS)),
                object(NSNumber::new_u32(AUDIO_BITRATE)),
            ],
        ))
    }
}

/// The format of the PCM the pipeline hands over: interleaved signed 16-bit stereo.
fn pcm_format() -> anyhow::Result<CFRetained<CMAudioFormatDescription>> {
    let bytes_per_frame = (CHANNELS * size_of::<i16>()) as u32;
    let description = AudioStreamBasicDescription {
        mSampleRate: f64::from(SAMPLE_RATE),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: CHANNELS as u32,
        mBitsPerChannel: 16,
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
    anyhow::ensure!(
        status == 0 && !format.is_null(),
        "cannot describe the audio format ({status})"
    );
    // SAFETY: the function returned a +1 reference.
    Ok(unsafe { CFRetained::from_raw(NonNull::new_unchecked(format.cast_mut())) })
}

/// Wraps PCM in a sample buffer: one copy into memory Core Media owns.
fn pcm_sample(
    pcm: &[i16],
    format: &CMAudioFormatDescription,
    presentation: CMTime,
) -> anyhow::Result<CFRetained<CMSampleBuffer>> {
    let bytes = size_of_val(pcm);
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: Core Media allocates `bytes` of memory for the block.
    let status = unsafe {
        CMBlockBuffer::create_with_memory_block(
            None,
            std::ptr::null_mut(),
            bytes,
            None,
            std::ptr::null(),
            0,
            bytes,
            kCMBlockBufferAssureMemoryNowFlag,
            NonNull::from(&mut block),
        )
    };
    anyhow::ensure!(
        status == 0 && !block.is_null(),
        "cannot allocate audio memory ({status})"
    );
    // SAFETY: a +1 reference from the call above.
    let block = unsafe { CFRetained::from_raw(NonNull::new_unchecked(block)) };
    // SAFETY: the block holds exactly `bytes` bytes.
    let status = unsafe { CMBlockBuffer::replace_data_bytes(NonNull::from(pcm).cast(), &block, 0, bytes) };
    anyhow::ensure!(status == 0, "cannot copy audio ({status})");

    let frames = (pcm.len() / CHANNELS) as isize;
    let mut sample: *mut CMSampleBuffer = std::ptr::null_mut();
    // SAFETY: the block and the format describe the same data.
    let status = unsafe {
        CMAudioSampleBufferCreateReadyWithPacketDescriptions(
            None,
            &block,
            AsRef::<CMFormatDescription>::as_ref(format),
            frames,
            presentation,
            std::ptr::null(),
            NonNull::from(&mut sample),
        )
    };
    anyhow::ensure!(status == 0 && !sample.is_null(), "cannot wrap audio ({status})");
    // SAFETY: a +1 reference from the call above.
    Ok(unsafe { CFRetained::from_raw(NonNull::new_unchecked(sample)) })
}
