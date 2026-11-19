//! Captures audio with WASAPI. The mixing calculations are in `crate::recording::mixer`.
//! All inputs are handled as 48 kHz, stereo, 16-bit (conversion is left to Windows).
//!
//! Each input has a thread that sleeps until Windows signals that sound has arrived, so the sound reaches the mixer
//! as soon as Windows has it, and an input that receives nothing (an app or a device playing nothing) wakes its
//! thread only once a second.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Context;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient,
    IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVE_FORMAT_PCM,
    WAVEFORMATEX, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{HRESULT, HSTRING, Interface, PCWSTR, Ref, implement};

use super::super::com::ComApartment;
use crate::recording::mixer::{BufferPolicy, SourceBuffer};
use crate::recording::plan::{AudioSource, CHANNELS, SAMPLE_RATE};

/// How long an input waits for sound before looking anyway, which notices a device that is gone.
const IDLE_CHECK_MILLISECONDS: u32 = 1000;
/// Length of the Windows-side buffer (in 100 ns units).
const DEVICE_BUFFER_DURATION: i64 = 2_000_000;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Amount and size of what was captured. Recorded so we can check afterwards that audio was actually recorded.
#[derive(Default)]
struct Stats {
    samples: AtomicU64,
    peak: AtomicU32,
}

/// A Windows event, closed when dropped.
struct Event(HANDLE);

// SAFETY: an event can be set and waited for from any thread.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Event {
    /// An event that resets itself once a wait has seen it.
    fn new() -> anyhow::Result<Self> {
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.context("cannot create an event")?;
        Ok(Self(handle))
    }

    fn set(&self) {
        let _ = unsafe { SetEvent(self.0) };
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// An input opened and capturing. Fields are dropped in declaration order: the client goes before the event it
/// signals.
struct Opened {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    /// Signaled by Windows each time sound has arrived.
    arrived: Event,
}

/// Capture from one input source. Runs on a dedicated thread and stops when dropped.
pub struct AudioCapture {
    label: String,
    /// The capture thread puts sound in, and the writing side takes it out.
    buffer: Arc<Mutex<SourceBuffer>>,
    stats: Arc<Stats>,
    stop: Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

impl AudioCapture {
    pub fn open(spec: AudioSource) -> anyhow::Result<Self> {
        let label = spec.label();
        let buffer = Arc::new(Mutex::new(SourceBuffer::new(BufferPolicy::for_format(
            SAMPLE_RATE,
            CHANNELS,
        ))));
        let stats = Arc::new(Stats::default());
        let stop = Arc::new(Event::new()?);
        let (ready_sender, ready_receiver) = mpsc::channel();

        let thread = std::thread::Builder::new().name("audio".into()).spawn({
            let (label, buffer, stats, stop) = (label.clone(), buffer.clone(), stats.clone(), stop.clone());
            move || {
                let opened = ComApartment::enter().and_then(|com| Ok((com, open_client(&spec)?)));
                match opened {
                    Ok((_com, opened)) => {
                        let _ = ready_sender.send(Ok(()));
                        if let Err(error) = pump(&opened, &stop, &buffer, &stats) {
                            // Recording is not stopped. From now on this input is silent.
                            log::warn!("audio capture stopped ({label}): {error:#}");
                        }
                        unsafe {
                            let _ = opened.client.Stop();
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            }
        })?;

        ready_receiver
            .recv()
            .context("the audio capture thread is not responding")??;
        Ok(Self {
            label,
            buffer,
            stats,
            stop,
            thread: Some(thread),
        })
    }

    /// Writing starts now; drops what was captured before.
    pub fn start_writing(&mut self) {
        self.buffer.lock().unwrap().start_writing();
    }

    /// Adds this input's sound to `out`.
    pub fn mix_into(&mut self, out: &mut [i16]) {
        self.buffer.lock().unwrap().mix_into(out);
    }

    /// How many samples have been captured, and the loudest of them.
    #[cfg(test)]
    fn captured(&self) -> (u64, u32) {
        (
            self.stats.samples.load(Ordering::Relaxed),
            self.stats.peak.load(Ordering::Relaxed),
        )
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.set();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let seconds = milliseconds(self.stats.samples.load(Ordering::Relaxed) as usize) / 1000.0;
        let peak = self.stats.peak.load(Ordering::Relaxed) as f64 / i16::MAX as f64 * 100.0;
        let report = self.buffer.lock().unwrap().report();
        log::info!(
            "audio input {}: captured {seconds:.1} s, peak volume {peak:.1}%, {:.0} ms dropped when writing started, \
             delay when mixing began {}, {} frames dropped and {} repeated to follow the clock, \
             {} catch-ups dropping {:.0} ms, {} underruns missing {:.0} ms, {:.0} ms dropped over the cap",
            self.label,
            milliseconds(report.dropped_at_start),
            report
                .queued_at_start
                .map_or("(never mixed)".to_owned(), |queued| format!(
                    "{:.0} ms",
                    milliseconds(queued)
                )),
            report.frames_dropped,
            report.frames_repeated,
            report.catch_ups,
            milliseconds(report.caught_up),
            report.underruns,
            milliseconds(report.missing),
            milliseconds(report.overflow),
        );
    }
}

/// Duration of a number of samples (all channels combined) in milliseconds.
fn milliseconds(samples: usize) -> f64 {
    samples as f64 * 1000.0 / (SAMPLE_RATE as f64 * CHANNELS as f64)
}

fn open_client(spec: &AudioSource) -> anyhow::Result<Opened> {
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS as u16,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * CHANNELS as u32 * 2,
        nBlockAlign: CHANNELS as u16 * 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };

    unsafe {
        let (client, loopback): (IAudioClient, u32) = match spec {
            AudioSource::ProcessAudio(process_id) => {
                (activate_process_loopback(*process_id)?, AUDCLNT_STREAMFLAGS_LOOPBACK)
            }
            AudioSource::DeviceAudio(id) => (
                endpoint(id.as_deref(), true)?.Activate(CLSCTX_ALL, None)?,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
            ),
            AudioSource::Microphone(id) => {
                let device = endpoint(id.as_deref(), false)?;
                warn_if_muted(&device);
                (device.Activate(CLSCTX_ALL, None)?, 0)
            }
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                loopback
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                DEVICE_BUFFER_DURATION,
                0,
                &format,
                None,
            )
            .context("cannot initialize audio capture")?;
        let arrived = Event::new()?;
        client
            .SetEventHandle(arrived.0)
            .context("cannot be told when sound arrives")?;
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start().context("cannot start audio capture")?;
        Ok(Opened {
            client,
            capture,
            arrived,
        })
    }
}

/// The specified device. If `id` is `None` or the device cannot be found, the default device.
unsafe fn endpoint(id: Option<&str>, output: bool) -> anyhow::Result<IMMDevice> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        if let Some(id) = id {
            match enumerator.GetDevice(&HSTRING::from(id)) {
                Ok(device) => return Ok(device),
                Err(_) => log::warn!("Audio device {id} was not found; using the default device."),
            }
        }
        enumerator
            .GetDefaultAudioEndpoint(if output { eRender } else { eCapture }, eConsole)
            .context("there is no default audio device")
    }
}

/// If the microphone is muted on the Windows side only silence arrives, so notify the user so they can notice.
unsafe fn warn_if_muted(device: &IMMDevice) {
    unsafe {
        let muted = device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .and_then(|volume| volume.GetMute())
            .is_ok_and(|muted| muted.as_bool());
        if muted {
            log::warn!("The microphone is muted in Windows settings. Your own voice will not be recorded.");
        }
    }
}

/// A COM object that receives the activation-completed notification, so we can wait for completion.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler(Mutex<mpsc::Sender<()>>);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    fn ActivateCompleted(&self, _operation: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let _ = self.0.lock().unwrap().send(());
        Ok(())
    }
}

/// Creates a client that captures only the sound produced by one process and its descendants
/// (Windows 10 2004 or later).
unsafe fn activate_process_loopback(process_id: u32) -> anyhow::Result<IAudioClient> {
    /// A struct with the same layout as the PROPVARIANT (VT_BLOB) that carries the activation arguments.
    /// Its contents are only borrowed for the duration of the call,
    /// so the real type, which has cleanup logic, is not used.
    #[repr(C)]
    struct BlobVariant {
        kind: u16,
        reserved: [u16; 3],
        size: u32,
        data: *mut c_void,
    }

    unsafe {
        let mut parameters = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: process_id,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let variant = BlobVariant {
            kind: VT_BLOB.0,
            reserved: [0; 3],
            size: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            data: (&raw mut parameters).cast(),
        };

        let (done_sender, done_receiver) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = ActivationHandler(Mutex::new(done_sender)).into();
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some((&raw const variant).cast()),
            &handler,
        )
        .context("cannot request capture of the app's audio")?;
        done_receiver
            .recv_timeout(ACTIVATION_TIMEOUT)
            .context("preparation for capturing the app's audio does not finish")?;

        let mut result = HRESULT(0);
        let mut interface = None;
        operation.GetActivateResult(&mut result, &mut interface)?;
        result.ok().context("cannot capture the app's audio")?;
        Ok(interface.context("cannot get the audio client")?.cast()?)
    }
}

/// Moves sound into the buffer each time it arrives, until `stop` is set.
fn pump(opened: &Opened, stop: &Event, buffer: &Mutex<SourceBuffer>, stats: &Stats) -> windows::core::Result<()> {
    let capture = &opened.capture;
    // When both are set, the first one wins: stopping goes before more sound.
    let events = [stop.0, opened.arrived.0];
    loop {
        let woken = unsafe { WaitForMultipleObjects(&events, false, IDLE_CHECK_MILLISECONDS) };
        if woken == WAIT_OBJECT_0 {
            return Ok(());
        }
        if woken == WAIT_FAILED {
            return Err(windows::core::Error::from_thread());
        }
        unsafe {
            while capture.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let samples = frames as usize * CHANNELS;
                {
                    let mut buffer = buffer.lock().unwrap();
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                        buffer.push_silence(samples);
                    } else {
                        let packet = std::slice::from_raw_parts(data.cast::<i16>(), samples);
                        let peak = packet.iter().map(|s| s.unsigned_abs() as u32).max().unwrap_or(0);
                        stats.peak.fetch_max(peak, Ordering::Relaxed);
                        buffer.push(packet);
                    }
                    stats.samples.fetch_add(samples as u64, Ordering::Relaxed);
                }
                capture.ReleaseBuffer(frames)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_LOOP, SND_MEMORY, SND_PURGE};

    use super::*;

    /// A 440 Hz tone in the format captured, as a WAV file of one second.
    fn tone() -> Vec<u8> {
        let samples: Vec<i16> = (0..SAMPLE_RATE)
            .flat_map(|index| {
                let value = (f64::from(index) * 440.0 / f64::from(SAMPLE_RATE) * std::f64::consts::TAU).sin();
                [(value * 8000.0) as i16; CHANNELS]
            })
            .collect();
        let data_length = (samples.len() * 2) as u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_length).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&(CHANNELS as u16).to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&(SAMPLE_RATE * CHANNELS as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&(CHANNELS as u16 * 2).to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_length.to_le_bytes());
        wav.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        wav
    }

    /// Plays a tone through the default output device, so it needs one:
    /// `cargo test -p meeting-assistant sound_played -- --ignored --nocapture`.
    #[test]
    #[ignore = "plays sound through the default output device"]
    fn sound_played_is_captured_from_the_output_device_and_from_the_process_that_plays_it() {
        let _com = ComApartment::enter().unwrap();
        let wav = tone();
        let playing = unsafe { PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_LOOP) };
        assert!(playing.as_bool(), "cannot play the tone");
        let sources = [
            AudioSource::DeviceAudio(None),
            AudioSource::ProcessAudio(std::process::id()),
        ];
        let captures: Vec<AudioCapture> = sources
            .into_iter()
            .map(|source| AudioCapture::open(source).unwrap())
            .collect();
        std::thread::sleep(Duration::from_secs(2));
        let captured: Vec<(u64, u32)> = captures.iter().map(AudioCapture::captured).collect();
        drop(captures);
        let _ = unsafe { PlaySoundW(PCWSTR::null(), None, SND_PURGE) };
        println!("captured from the device and from the process (samples, peak): {captured:?}");
        for (samples, peak) in captured {
            // At least one of the two seconds, at about the volume played.
            assert!(samples >= u64::from(SAMPLE_RATE) * CHANNELS as u64, "{samples} samples");
            assert!(peak > 4000, "peak {peak}");
        }
    }
}
