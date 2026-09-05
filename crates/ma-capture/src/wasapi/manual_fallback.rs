//! The endpoint paths (Windows only): system loopback on the default render endpoint, and the
//! manual Device-mode path on a capture endpoint chosen by the user or the default one. Both
//! stay constructible whatever the process-loopback outcome.

use super::process_loopback::{failed, ComGuard, SampleLayout, WindowsStream, BUFFER_DURATION_HNS};
use super::{ActivationError, AudioStream, StreamFormat};
use windows::core::HSTRING;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    WAVE_FORMAT_PCM,
};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

/// `E_NOTFOUND` from `IMMDeviceEnumerator::GetDefaultAudioEndpoint` when no endpoint exists.
const E_NOTFOUND: i32 = 0x8007_0490_u32 as i32;

/// Reads the mix format into a stream format and a sample layout.
///
/// # Safety
/// `format` must point at a `WAVEFORMATEX` (possibly `WAVEFORMATEXTENSIBLE`) returned by WASAPI.
unsafe fn describe(format: *const WAVEFORMATEX) -> Option<(StreamFormat, SampleLayout)> {
    let base = *format;
    let stream = StreamFormat {
        sample_rate: base.nSamplesPerSec,
        channels: base.nChannels,
    };
    let (tag, sub_pcm, sub_float) = if base.wFormatTag as u32 == WAVE_FORMAT_EXTENSIBLE {
        let ext = *(format as *const WAVEFORMATEXTENSIBLE);
        let sub = ext.SubFormat;
        (
            0,
            sub == KSDATAFORMAT_SUBTYPE_PCM,
            sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        )
    } else {
        (base.wFormatTag as u32, false, false)
    };
    let layout = match (tag, sub_pcm, sub_float, base.wBitsPerSample) {
        (WAVE_FORMAT_PCM, _, _, 16) | (_, true, _, 16) => SampleLayout::Pcm16,
        (WAVE_FORMAT_PCM, _, _, 24) | (_, true, _, 24) => SampleLayout::Pcm24,
        (WAVE_FORMAT_PCM, _, _, 32) | (_, true, _, 32) => SampleLayout::Pcm32,
        (WAVE_FORMAT_IEEE_FLOAT, _, _, 32) | (_, _, true, 32) => SampleLayout::Float32,
        _ => return None,
    };
    Some((stream, layout))
}

/// Opens a shared-mode stream on an endpoint. `loopback` selects the default render endpoint in
/// loopback mode (system loopback); otherwise a capture endpoint (`endpoint_id` or the default).
pub(super) fn open_endpoint(
    endpoint_id: Option<&str>,
    loopback: bool,
) -> Result<Box<dyn AudioStream>, ActivationError> {
    let com = ComGuard::init();
    // SAFETY: standard MMDevice enumerator creation.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(failed)?;
    let device = match (endpoint_id, loopback) {
        (Some(id), false) => unsafe { enumerator.GetDevice(&HSTRING::from(id)) },
        (_, true) => unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) },
        (None, false) => unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) },
    }
    .map_err(|e| {
        if e.code().0 == E_NOTFOUND {
            ActivationError::NoEndpoint
        } else {
            failed(e)
        }
    })?;
    // SAFETY: activates the audio client interface on the endpoint.
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(failed)?;
    // SAFETY: the mix format is CoTaskMem-allocated and freed below after use.
    let mix = unsafe { client.GetMixFormat() }.map_err(failed)?;
    // SAFETY: plain-field copy taken before the buffer is freed, for the error path below.
    let mix_base = unsafe { *mix };
    let described = unsafe { describe(mix) };
    let init = described.map(|(format, layout)| {
        let flags = if loopback {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
        // SAFETY: shared mode with the endpoint's own mix format.
        let r = unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                BUFFER_DURATION_HNS,
                0,
                mix,
                None,
            )
        };
        (format, layout, r)
    });
    // SAFETY: frees the buffer GetMixFormat allocated.
    unsafe { CoTaskMemFree(Some(mix.cast())) };
    let (format, layout, init_result) =
        init.ok_or(ActivationError::UnsupportedFormat(StreamFormat {
            sample_rate: mix_base.nSamplesPerSec,
            channels: mix_base.nChannels,
        }))?;
    init_result.map_err(failed)?;
    let stream = WindowsStream::start(client, format, layout, com)?;
    Ok(Box::new(stream))
}
