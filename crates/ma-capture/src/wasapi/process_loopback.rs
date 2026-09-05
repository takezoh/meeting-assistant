//! Live WASAPI activation (Windows only): process loopback through `ActivateAudioInterfaceAsync`
//! with `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, plus the shared capture stream over
//! `IAudioClient` / `IAudioCaptureClient` that every activation path returns.

use super::manual_fallback::open_endpoint;
use super::{
    ActivationBackend, ActivationError, AudioStream, LoopbackTarget, StreamFormat, StreamRead,
};
use std::mem::ManuallyDrop;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;
use windows::core::{implement, Interface, Ref, HRESULT};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_UNSUPPORTED_FORMAT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VT_BLOB;

/// `AUDCLNT_E_DEVICE_INVALIDATED`: the endpoint went away mid-stream.
const AUDCLNT_E_DEVICE_INVALIDATED: HRESULT = HRESULT(0x8889_0004_u32 as i32);
/// `E_NOTIMPL` / `AUDCLNT_E_UNSUPPORTED_FORMAT` from the virtual device mean the activation type
/// is not available for this host or process.
const E_NOTIMPL: HRESULT = HRESULT(0x8000_4001_u32 as i32);
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(5);
/// 200 ms shared-mode buffer, in 100 ns units.
pub(super) const BUFFER_DURATION_HNS: i64 = 2_000_000;

/// Keeps COM initialised for the lifetime of the backend.
pub(super) struct ComGuard {
    owns: bool,
}

impl ComGuard {
    pub(super) fn init() -> Self {
        // SAFETY: plain COM initialisation on the calling thread; RPC_E_CHANGED_MODE means another
        // component already initialised it with another model, which is fine for our calls.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self { owns: hr.is_ok() }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owns {
            // SAFETY: balanced with the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
    }
}

/// How the bytes of a capture packet are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SampleLayout {
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
}

/// One running `IAudioClient` capture stream.
pub(super) struct WindowsStream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    format: StreamFormat,
    layout: SampleLayout,
    discontinuities: u32,
    _com: ComGuard,
}

impl WindowsStream {
    pub(super) fn start(
        client: IAudioClient,
        format: StreamFormat,
        layout: SampleLayout,
        com: ComGuard,
    ) -> Result<Self, ActivationError> {
        // SAFETY: the client is initialised by the caller.
        let capture: IAudioCaptureClient = unsafe { client.GetService() }.map_err(failed)?;
        unsafe { client.Start() }.map_err(failed)?;
        Ok(Self {
            client,
            capture,
            format,
            layout,
            discontinuities: 0,
            _com: com,
        })
    }

    #[allow(clippy::chunks_exact_to_as_chunks)] // as_chunks needs Rust 1.88; rust-version is 1.85
    fn convert(&self, bytes: &[u8], frames: usize, silent: bool) -> Vec<i16> {
        let n = frames * self.format.channels as usize;
        if silent {
            return vec![0; n];
        }
        match self.layout {
            SampleLayout::Pcm16 => bytes
                .chunks_exact(2)
                .take(n)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect(),
            SampleLayout::Pcm24 => bytes
                .chunks_exact(3)
                .take(n)
                .map(|b| i16::from_le_bytes([b[1], b[2]]))
                .collect(),
            SampleLayout::Pcm32 => bytes
                .chunks_exact(4)
                .take(n)
                .map(|b| i16::from_le_bytes([b[2], b[3]]))
                .collect(),
            SampleLayout::Float32 => bytes
                .chunks_exact(4)
                .take(n)
                .map(|b| {
                    let f = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    (f.clamp(-1.0, 1.0) * 32_767.0).round() as i16
                })
                .collect(),
        }
    }
}

impl Drop for WindowsStream {
    fn drop(&mut self) {
        // SAFETY: stopping a started client; errors are irrelevant at teardown.
        let _ = unsafe { self.client.Stop() };
    }
}

impl AudioStream for WindowsStream {
    fn format(&self) -> StreamFormat {
        self.format
    }

    fn take_discontinuities(&mut self) -> u32 {
        std::mem::take(&mut self.discontinuities)
    }

    fn read(&mut self) -> StreamRead {
        loop {
            // SAFETY: capture client obtained from the started client.
            let next = match unsafe { self.capture.GetNextPacketSize() } {
                Ok(n) => n,
                Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => return StreamRead::Lost,
                Err(_) => return StreamRead::Ended,
            };
            if next == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;
            // SAFETY: out-pointers as the API requires; the buffer is released below.
            if let Err(e) = unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
            } {
                return if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
                    StreamRead::Lost
                } else {
                    StreamRead::Ended
                };
            }
            let bytes_per_sample = match self.layout {
                SampleLayout::Pcm16 => 2,
                SampleLayout::Pcm24 => 3,
                SampleLayout::Pcm32 | SampleLayout::Float32 => 4,
            };
            let len = frames as usize * self.format.channels as usize * bytes_per_sample;
            // SAFETY: GetBuffer returned `frames` frames of the initialised format at `data`.
            let bytes: &[u8] = if data.is_null() || len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(data, len) }
            };
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            if flags & (AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0 {
                self.discontinuities = self.discontinuities.saturating_add(1);
            }
            let samples = self.convert(bytes, frames as usize, silent);
            // SAFETY: releases exactly the frames GetBuffer handed out.
            let _ = unsafe { self.capture.ReleaseBuffer(frames) };
            return StreamRead::Samples(samples);
        }
    }
}

pub(super) fn failed(e: windows::core::Error) -> ActivationError {
    ActivationError::Failed { code: e.code().0 }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Completion {
    done: Mutex<Option<Sender<()>>>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        if let Some(tx) = self.done.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// The format requested from the process-loopback virtual device: 16-bit PCM, 48 kHz stereo.
/// The virtual device has no mix format of its own; the engine converts into what we ask for.
fn loopback_request_format() -> WAVEFORMATEX {
    let channels: u16 = 2;
    let rate: u32 = 48_000;
    let bits: u16 = 16;
    let block_align = channels * bits / 8;
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: bits,
        cbSize: 0,
    }
}

/// The live activation backend.
pub struct WindowsActivationBackend;

impl Default for WindowsActivationBackend {
    fn default() -> Self {
        Self
    }
}

impl WindowsActivationBackend {
    pub fn new() -> Self {
        Self
    }

    fn activate_client(target: LoopbackTarget) -> Result<IAudioClient, ActivationError> {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            ..Default::default()
        };
        params.Anonymous.ProcessLoopbackParams = AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
            TargetProcessId: target.pid,
            ProcessLoopbackMode: if target.include_process_tree {
                PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
            } else {
                PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
            },
        };
        let mut prop = PROPVARIANT::default();
        prop.Anonymous.Anonymous = ManuallyDrop::new(PROPVARIANT_0_0 {
            vt: VT_BLOB,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: PROPVARIANT_0_0_0 {
                blob: BLOB {
                    cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                    pBlobData: (&mut params as *mut AUDIOCLIENT_ACTIVATION_PARAMS).cast::<u8>(),
                },
            },
        });
        let (tx, rx) = channel::<()>();
        let handler: IActivateAudioInterfaceCompletionHandler = Completion {
            done: Mutex::new(Some(tx)),
        }
        .into();
        // SAFETY: `params` outlives the call; the PROPVARIANT points at it as a BLOB.
        let operation = unsafe {
            ActivateAudioInterfaceAsync(
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
                &IAudioClient::IID,
                Some(&prop),
                &handler,
            )
        }
        .map_err(|e| {
            if e.code() == E_NOTIMPL {
                ActivationError::Unavailable
            } else {
                failed(e)
            }
        })?;
        if rx.recv_timeout(ACTIVATION_TIMEOUT).is_err() {
            return Err(ActivationError::Unavailable);
        }
        let mut hr = HRESULT(0);
        let mut activated: Option<windows::core::IUnknown> = None;
        // SAFETY: out-parameters of the completed operation.
        unsafe { operation.GetActivateResult(&mut hr, &mut activated) }.map_err(failed)?;
        if hr.is_err() {
            return Err(if hr == E_NOTIMPL || hr == AUDCLNT_E_UNSUPPORTED_FORMAT {
                ActivationError::Unavailable
            } else {
                ActivationError::Failed { code: hr.0 }
            });
        }
        activated
            .ok_or(ActivationError::Unavailable)?
            .cast::<IAudioClient>()
            .map_err(failed)
    }
}

impl ActivationBackend for WindowsActivationBackend {
    fn activate_process_loopback(
        &mut self,
        target: LoopbackTarget,
    ) -> Result<Box<dyn AudioStream>, ActivationError> {
        let com = ComGuard::init();
        let client = Self::activate_client(target)?;
        let format = loopback_request_format();
        // SAFETY: shared-mode initialisation with the request format; loopback flag is required
        // for the process-loopback virtual device.
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                BUFFER_DURATION_HNS,
                0,
                &format,
                None,
            )
        }
        .map_err(|e| {
            if e.code() == AUDCLNT_E_UNSUPPORTED_FORMAT {
                ActivationError::UnsupportedFormat(StreamFormat {
                    sample_rate: format.nSamplesPerSec,
                    channels: format.nChannels,
                })
            } else {
                failed(e)
            }
        })?;
        let stream = WindowsStream::start(
            client,
            StreamFormat {
                sample_rate: format.nSamplesPerSec,
                channels: format.nChannels,
            },
            SampleLayout::Pcm16,
            com,
        )?;
        Ok(Box::new(stream))
    }

    fn activate_system_loopback(&mut self) -> Result<Box<dyn AudioStream>, ActivationError> {
        open_endpoint(None, true)
    }

    fn open_device(
        &mut self,
        endpoint_id: Option<&str>,
    ) -> Result<Box<dyn AudioStream>, ActivationError> {
        open_endpoint(endpoint_id, false)
    }
}
