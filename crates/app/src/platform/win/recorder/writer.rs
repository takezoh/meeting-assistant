//! Writes H.264/AAC fragmented MP4 with the Media Foundation SinkWriter.
//! A fragmented MP4 is written to disk fragment by fragment,
//! so if the process dies mid-recording the part already written survives.
//!
//! The sink writer picks the H.264 encoder: a hardware encoder of the GPU when there is one, of whichever vendor,
//! otherwise Windows' software encoder. Nothing here is specific to a vendor.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use windows::Win32::Foundation::E_NOTIMPL;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::{GUID, HSTRING, Interface, Ref, implement};

use super::scale::{Holder, Picture, PictureFormat};
use crate::recording::plan::{CHANNELS, MediaFormat, SAMPLE_RATE};

const AAC_BYTES_PER_SECOND: u32 = 16_000; // 128 kbps
const BYTES_PER_FRAME: u32 = CHANNELS as u32 * 2;
const KEYFRAME_INTERVAL_SECONDS: u32 = 8;

pub struct Mp4Writer {
    writer: IMFSinkWriter,
    /// `None` for a file without video.
    video_stream: Option<u32>,
    /// One stream per audio track, in the file's order.
    audio_streams: Vec<u32>,
    /// The transforms that encode the video, as the log names them.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "read by the tests that check which encoder was chosen")
    )]
    encoder: String,
    // Manager for the D3D device the SinkWriter references. It must be kept alive while writing.
    _device_manager: IMFDXGIDeviceManager,
}

impl Mp4Writer {
    /// `device` must be the same device that created the pictures passed in, which are in `pictures` format.
    pub fn create(
        path: &Path,
        device: &ID3D11Device,
        format: &MediaFormat,
        pictures: PictureFormat,
    ) -> anyhow::Result<Self> {
        Self::open(path, device, format, pictures, true)
    }

    /// The same, with the hardware encoders left out, as on a PC that has none.
    #[cfg(test)]
    pub fn create_in_software(
        path: &Path,
        device: &ID3D11Device,
        format: &MediaFormat,
        pictures: PictureFormat,
    ) -> anyhow::Result<Self> {
        Self::open(path, device, format, pictures, false)
    }

    fn open(
        path: &Path,
        device: &ID3D11Device,
        format: &MediaFormat,
        pictures: PictureFormat,
        hardware: bool,
    ) -> anyhow::Result<Self> {
        unsafe {
            let mut reset_token = 0u32;
            let mut device_manager = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut device_manager)?;
            let device_manager: IMFDXGIDeviceManager = device_manager.context("cannot create the device manager")?;
            device_manager.ResetDevice(device, reset_token)?;

            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 4)?;
            let attributes: IMFAttributes = attributes.context("cannot create attributes")?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, u32::from(hardware))?;
            attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &device_manager)?;
            attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attributes)
                .with_context(|| format!("cannot create the output file: {}", path.display()))?;

            let video_stream = if format.has_video {
                Some(add_video_stream(&writer, format, pictures).context("cannot configure the video encoder")?)
            } else {
                None
            };
            // Streams become tracks in the order they are added.
            let mut audio_streams = Vec::new();
            for track in &format.audio_tracks {
                audio_streams.push(
                    add_audio_stream(&writer).with_context(|| {
                        format!("cannot configure the audio encoder for the {} track", track.label())
                    })?,
                );
            }
            writer.BeginWriting().context("cannot begin writing")?;
            let encoder =
                video_stream.map_or_else(|| "no video".to_owned(), |stream| describe_transforms(&writer, stream));
            if video_stream.is_some() {
                log::info!("video is encoded by: {encoder}");
            }

            Ok(Self {
                writer,
                video_stream,
                audio_streams,
                encoder,
                _device_manager: device_manager,
            })
        }
    }

    /// The transforms that encode the video: what each is, whether it runs on the GPU and what it takes in.
    #[cfg(test)]
    pub fn encoder(&self) -> &str {
        &self.encoder
    }

    /// `time` and `duration` are in 100 ns units. The picture stays held until the encoder lets go of it. Does nothing
    /// for a file without video.
    pub fn write_video(&self, picture: &Picture, time: i64, duration: i64) -> anyhow::Result<()> {
        let Some(stream) = self.video_stream else { return Ok(()) };
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, picture.texture(), 0, false)?;
            let length = buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?;
            buffer.SetCurrentLength(length)?;
            // A tracked sample calls back once nothing refers to it any more: the encoder is done with the texture.
            let sample = MFCreateTrackedSample()?;
            let release: IMFAsyncCallback = LetGo(Mutex::new(Some(picture.hold()))).into();
            sample.SetAllocator(&release, None)?;
            self.write(stream, &sample.cast()?, &buffer, time, duration)
        }
    }

    /// Writes to the audio track at `track`. `pcm` is 16-bit interleaved PCM.
    pub fn write_audio(&self, track: usize, pcm: &[i16], time: i64, duration: i64) -> anyhow::Result<()> {
        let stream = *self
            .audio_streams
            .get(track)
            .context("the file has no such audio track")?;
        if pcm.is_empty() {
            return Ok(());
        }
        unsafe {
            let byte_length = size_of_val(pcm) as u32;
            let buffer = MFCreateMemoryBuffer(byte_length)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr().cast::<u8>(), data, byte_length as usize);
            buffer.Unlock()?;
            buffer.SetCurrentLength(byte_length)?;
            self.write(stream, &MFCreateSample()?, &buffer, time, duration)
        }
    }

    /// Writes out the rest and finalizes the file.
    pub fn finalize(self) -> anyhow::Result<()> {
        self.log_statistics();
        unsafe { self.writer.Finalize() }.context("cannot finalize the file")
    }

    /// How far each stream got before finalizing. Samples received but not yet encoded are what the encoder has
    /// fallen behind by.
    fn log_statistics(&self) {
        let audio = self
            .audio_streams
            .iter()
            .enumerate()
            .map(|(track, stream)| (format!("audio track {}", track + 1), *stream));
        for (name, stream) in self
            .video_stream
            .map(|stream| ("video".to_owned(), stream))
            .into_iter()
            .chain(audio)
        {
            let mut statistics = MF_SINK_WRITER_STATISTICS {
                cb: size_of::<MF_SINK_WRITER_STATISTICS>() as u32,
                ..Default::default()
            };
            if unsafe { self.writer.GetStatistics(stream, &mut statistics) }.is_ok() {
                log::info!(
                    "{name} stream before finalizing: {} samples received, {} encoded, {} written, {} bytes queued",
                    statistics.qwNumSamplesReceived,
                    statistics.qwNumSamplesEncoded,
                    statistics.qwNumSamplesProcessed,
                    statistics.dwByteCountQueued,
                );
            }
        }
    }

    unsafe fn write(
        &self,
        stream: u32,
        sample: &IMFSample,
        buffer: &IMFMediaBuffer,
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()> {
        unsafe {
            sample.AddBuffer(buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;
            self.writer.WriteSample(stream, sample)?;
            Ok(())
        }
    }
}

/// Lets go of a picture once the sample made from it is released. Called on a Media Foundation work queue thread;
/// if it is never called, dropping it lets go all the same.
#[implement(IMFAsyncCallback)]
struct LetGo(Mutex<Option<Holder>>);

impl IMFAsyncCallback_Impl for LetGo_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> windows::core::Result<()> {
        // The default work queue.
        Err(E_NOTIMPL.into())
    }

    fn Invoke(&self, _result: Ref<IMFAsyncResult>) -> windows::core::Result<()> {
        self.0.lock().unwrap().take();
        Ok(())
    }
}

unsafe fn add_video_stream(
    writer: &IMFSinkWriter,
    video: &MediaFormat,
    pictures: PictureFormat,
) -> windows::core::Result<u32> {
    unsafe {
        let describe = |subtype: &GUID| -> windows::core::Result<IMFMediaType> {
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack(video.width, video.height))?;
            media_type.SetUINT64(&MF_MT_FRAME_RATE, pack(video.framerate, 1))?;
            media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            // The colors the pictures are converted to, so that players read them back the same way.
            media_type.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
            media_type.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
            media_type.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
            media_type.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
            Ok(media_type)
        };

        let output = describe(&MFVideoFormat_H264)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, video.fallback_bitrate)?;
        output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)?;
        // Keyframes are large, so space them out. The file gets smaller at the cost of seek granularity.
        output.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, video.framerate * KEYFRAME_INTERVAL_SECONDS)?;
        let stream = writer.AddStream(&output)?;
        let input = describe(match pictures {
            PictureFormat::Nv12 => &MFVideoFormat_NV12,
            PictureFormat::Bgra => &MFVideoFormat_ARGB32,
        })?;

        // Using a quality target makes files much smaller for displays with little motion.
        let mut quality = None;
        MFCreateAttributes(&mut quality, 3)?;
        let quality: IMFAttributes = quality.unwrap();
        quality.SetUINT32(&CODECAPI_AVEncMPVGOPSize, video.framerate * KEYFRAME_INTERVAL_SECONDS)?;
        quality.SetUINT32(
            &CODECAPI_AVEncCommonRateControlMode,
            eAVEncCommonRateControlMode_Quality.0 as u32,
        )?;
        quality.SetUINT32(&CODECAPI_AVEncCommonQuality, video.quality)?;
        if let Err(error) = writer.SetInputMediaType(stream, &input, &quality) {
            log::warn!("The encoder does not support a quality target; recording at a fixed average bitrate: {error}");
            writer.SetInputMediaType(stream, &input, None)?;
        }
        Ok(stream)
    }
}

unsafe fn add_audio_stream(writer: &IMFSinkWriter) -> windows::core::Result<u32> {
    unsafe {
        let describe = |subtype: &GUID| -> windows::core::Result<IMFMediaType> {
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)?;
            media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
            Ok(media_type)
        };

        let output = describe(&MFAudioFormat_AAC)?;
        output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AAC_BYTES_PER_SECOND)?;
        let stream = writer.AddStream(&output)?;

        let input = describe(&MFAudioFormat_PCM)?;
        input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, BYTES_PER_FRAME)?;
        input.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, SAMPLE_RATE * BYTES_PER_FRAME)?;
        input.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        writer.SetInputMediaType(stream, &input, None)?;
        Ok(stream)
    }
}

/// The transforms the sink writer placed in front of the file for one stream, in order, for the log: what each one
/// is, whether it runs on the GPU, and what it takes in.
unsafe fn describe_transforms(writer: &IMFSinkWriter, stream: u32) -> String {
    unsafe {
        let Ok(writer) = writer.cast::<IMFSinkWriterEx>() else {
            return "(not reported)".to_owned();
        };
        let mut transforms = Vec::new();
        for index in 0.. {
            let mut category = GUID::zeroed();
            let mut transform = None;
            if writer
                .GetTransformForStream(stream, index, Some(&mut category), &mut transform)
                .is_err()
            {
                break;
            }
            let Some(transform) = transform else { break };
            let role = if category == MFT_CATEGORY_VIDEO_ENCODER {
                "encoder"
            } else if category == MFT_CATEGORY_VIDEO_PROCESSOR {
                "converter"
            } else {
                "transform"
            };
            let attributes = transform.GetAttributes().ok();
            let name = attributes
                .as_ref()
                .and_then(|attributes| string_attribute(attributes, &MFT_FRIENDLY_NAME_Attribute))
                .unwrap_or_else(|| "unnamed".to_owned());
            // Only hardware transforms carry a hardware URL.
            let place = match &attributes {
                Some(attributes) if attributes.GetStringLength(&MFT_ENUM_HARDWARE_URL_Attribute).is_ok() => "hardware",
                _ => "software",
            };
            let input = transform
                .GetInputCurrentType(0)
                .and_then(|media_type| media_type.GetGUID(&MF_MT_SUBTYPE))
                .map_or_else(|_| "?".to_owned(), |subtype| subtype_name(&subtype));
            transforms.push(format!("{role} \"{name}\" ({place}, takes {input})"));
        }
        if transforms.is_empty() {
            return "(no transform reported)".to_owned();
        }
        transforms.join(" -> ")
    }
}

unsafe fn string_attribute(attributes: &IMFAttributes, key: &GUID) -> Option<String> {
    unsafe {
        let length = attributes.GetStringLength(key).ok()? as usize;
        let mut value = vec![0u16; length + 1];
        attributes.GetString(key, &mut value, None).ok()?;
        Some(String::from_utf16_lossy(&value[..length]))
    }
}

fn subtype_name(subtype: &GUID) -> String {
    let known = [
        (MFVideoFormat_NV12, "NV12"),
        (MFVideoFormat_ARGB32, "ARGB32"),
        (MFVideoFormat_RGB32, "RGB32"),
        (MFVideoFormat_H264, "H.264"),
    ];
    known
        .iter()
        .find(|(guid, _)| guid == subtype)
        .map_or_else(|| format!("{subtype:?}"), |(_, name)| (*name).to_owned())
}

/// The format in which Media Foundation packs two 32-bit values into one attribute.
fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}
