//! Writes H.264/AAC fragmented MP4 with the Media Foundation SinkWriter.
//! A fragmented MP4 is written to disk fragment by fragment,
//! so if the process dies mid-recording the part already written survives.

use std::path::Path;

use anyhow::Context;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::{GUID, HSTRING, Interface};

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
    // Manager for the D3D device the SinkWriter references. It must be kept alive while writing.
    _device_manager: IMFDXGIDeviceManager,
}

impl Mp4Writer {
    /// `device` must be the same device that created the textures passed in.
    pub fn create(path: &Path, device: &ID3D11Device, format: &MediaFormat) -> anyhow::Result<Self> {
        unsafe {
            let mut reset_token = 0u32;
            let mut device_manager = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut device_manager)?;
            let device_manager: IMFDXGIDeviceManager = device_manager.context("cannot create the device manager")?;
            device_manager.ResetDevice(device, reset_token)?;

            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 4)?;
            let attributes: IMFAttributes = attributes.context("cannot create attributes")?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &device_manager)?;
            attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attributes)
                .with_context(|| format!("cannot create the output file: {}", path.display()))?;

            let video_stream = if format.has_video {
                Some(add_video_stream(&writer, format).context("cannot configure the video encoder")?)
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

            Ok(Self {
                writer,
                video_stream,
                audio_streams,
                _device_manager: device_manager,
            })
        }
    }

    /// `time` and `duration` are in 100 ns units. Do not modify the texture until writing has finished. Does nothing
    /// for a file without video.
    pub fn write_video(&self, texture: &ID3D11Texture2D, time: i64, duration: i64) -> anyhow::Result<()> {
        let Some(stream) = self.video_stream else { return Ok(()) };
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)?;
            let length = buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?;
            buffer.SetCurrentLength(length)?;
            self.write(stream, &buffer, time, duration)
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
            self.write(stream, &buffer, time, duration)
        }
    }

    /// Writes out the rest and finalizes the file.
    pub fn finalize(self) -> anyhow::Result<()> {
        unsafe { self.writer.Finalize() }.context("cannot finalize the file")
    }

    unsafe fn write(&self, stream: u32, buffer: &IMFMediaBuffer, time: i64, duration: i64) -> anyhow::Result<()> {
        unsafe {
            let sample = MFCreateSample()?;
            sample.AddBuffer(buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;
            self.writer.WriteSample(stream, &sample)?;
            Ok(())
        }
    }
}

unsafe fn add_video_stream(writer: &IMFSinkWriter, video: &MediaFormat) -> windows::core::Result<u32> {
    unsafe {
        let describe = |subtype: &GUID| -> windows::core::Result<IMFMediaType> {
            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack(video.width, video.height))?;
            media_type.SetUINT64(&MF_MT_FRAME_RATE, pack(video.framerate, 1))?;
            media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            Ok(media_type)
        };

        let output = describe(&MFVideoFormat_H264)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, video.fallback_bitrate)?;
        output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)?;
        // Keyframes are large, so space them out. The file gets smaller at the cost of seek granularity.
        output.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, video.framerate * KEYFRAME_INTERVAL_SECONDS)?;
        let stream = writer.AddStream(&output)?;
        let input = describe(&MFVideoFormat_ARGB32)?;

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

/// The format in which Media Foundation packs two 32-bit values into one attribute.
fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}
