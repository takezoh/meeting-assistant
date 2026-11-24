//! The microphone on macOS, through AVAudioEngine's input node. Needs the microphone permission only, not the
//! screen recording permission, so it also serves recordings made by the browser extension.

use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use objc2_avf_audio::{AVAudioEngine, AVAudioPCMBuffer, AVAudioTime};

use crate::recording::mixer::{BufferPolicy, SourceBuffer};
use crate::recording::pcm::Resampler;
use crate::recording::plan::{CHANNELS, SAMPLE_RATE};

/// Frames the engine hands over per call, about 100 ms.
const TAP_FRAMES: u32 = 4800;

pub struct Microphone {
    engine: Retained<AVAudioEngine>,
    sound: Arc<Mutex<SourceBuffer>>,
    _tap: RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>,
}

impl Microphone {
    /// Starts the default input device. The device the meeting app uses is not selected yet.
    pub fn start() -> anyhow::Result<Self> {
        // SAFETY: AVAudioEngine calls on a fresh engine; the tap block is kept for the engine's life.
        unsafe {
            let engine = AVAudioEngine::new();
            let input = engine.inputNode();
            let format = input.outputFormatForBus(0);
            let rate = format.sampleRate();
            anyhow::ensure!(
                rate > 0.0 && format.channelCount() > 0,
                "no microphone is available (or the microphone permission was not given)"
            );

            let sound = Arc::new(Mutex::new(SourceBuffer::new(BufferPolicy::for_format(
                SAMPLE_RATE,
                CHANNELS,
            ))));
            let resampler = Mutex::new(Resampler::new(rate.round() as u32, SAMPLE_RATE));
            let tap = RcBlock::new({
                let sound = sound.clone();
                move |buffer: NonNull<AVAudioPCMBuffer>, _time: NonNull<AVAudioTime>| {
                    let Some(channels) = channels_of(buffer.as_ref()) else {
                        return;
                    };
                    let pcm = resampler.lock().unwrap().resample_to_i16(&channels, CHANNELS);
                    sound.lock().unwrap().push(&pcm);
                }
            });
            input.installTapOnBus_bufferSize_format_block(0, TAP_FRAMES, None, RcBlock::as_ptr(&tap));
            engine.prepare();
            engine
                .startAndReturnError()
                .map_err(|error| anyhow::anyhow!("cannot start the microphone: {error}"))?;
            // A microphone macOS does not let this app use still starts, and gives silence. The permission is asked
            // while the engine starts, so it is known by now.
            if let Err(error) = microphone_allowed() {
                engine.stop();
                input.removeTapOnBus(0);
                return Err(error);
            }
            Ok(Self {
                engine,
                sound,
                _tap: tap,
            })
        }
    }

    /// Writing starts now; drops the sound captured before.
    pub fn start_writing(&self) {
        self.sound.lock().unwrap().start_writing();
    }

    pub fn mix_into(&self, out: &mut [i16]) {
        self.sound.lock().unwrap().mix_into(out);
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        // SAFETY: stopping the engine this value started.
        unsafe {
            self.engine.stop();
            self.engine.inputNode().removeTapOnBus(0);
        }
    }
}

/// Errors if macOS does not let this app use the microphone.
fn microphone_allowed() -> anyhow::Result<()> {
    // SAFETY: a class method given the framework's own media type constant.
    let status = unsafe {
        let audio = AVMediaTypeAudio.ok_or_else(|| anyhow::anyhow!("AVFoundation has no audio media type"))?;
        AVCaptureDevice::authorizationStatusForMediaType(audio)
    };
    anyhow::ensure!(
        status != AVAuthorizationStatus::Denied && status != AVAuthorizationStatus::Restricted,
        "macOS does not let Meeting Assistant use the microphone. Allow it in System Settings > Privacy & Security > Microphone"
    );
    Ok(())
}

/// The float samples of a tapped buffer, one slice per channel, valid while the buffer is.
///
/// # Safety
/// The buffer must be the one the engine passed to the tap.
unsafe fn channels_of(buffer: &AVAudioPCMBuffer) -> Option<Vec<&[f32]>> {
    // SAFETY: reading the buffer's description and its float channel pointers.
    unsafe {
        let frames = buffer.frameLength() as usize;
        let data = buffer.floatChannelData();
        if data.is_null() || frames == 0 {
            return None;
        }
        let channels = buffer.format().channelCount() as usize;
        // A tap delivers deinterleaved float buffers, one pointer per channel.
        Some(
            (0..channels)
                .map(|channel| std::slice::from_raw_parts((*data.add(channel)).as_ptr(), frames))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a microphone and the microphone permission for the process that runs the test, so it is run by hand:
    /// `cargo test -p meeting-assistant microphone -- --ignored --nocapture`. Nothing is kept.
    #[test]
    #[ignore = "uses the real microphone"]
    fn the_microphone_delivers_sound_in_the_mixer_format() {
        let microphone = Microphone::start().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        // One second of the mixer's format.
        let mut out = vec![0i16; SAMPLE_RATE as usize * CHANNELS];
        microphone.mix_into(&mut out);
        drop(microphone);

        let peak = out.iter().map(|sample| sample.unsigned_abs()).max().unwrap_or(0);
        let filled = out.iter().rposition(|&sample| sample != 0).map_or(0, |at| at + 1);
        println!("peak {peak}, {filled} of {} samples", out.len());
        assert!(peak > 0, "the microphone delivered only silence");
    }
}
