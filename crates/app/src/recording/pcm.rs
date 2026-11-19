//! Turning captured audio into the format the mixer works in. No side effects.
//!
//! Some capture APIs deliver 32-bit floating point samples, one buffer per channel. The mixer works on interleaved
//! 16-bit stereo ([`super::plan::CHANNELS`]); a mono source is played on both sides.

/// Interleaves float channels (each sample in -1.0..=1.0) into 16-bit samples with `out_channels` channels.
///
/// One input channel is copied to every output channel; more input channels than output channels are cut. Samples
/// beyond the shortest channel are dropped, and values out of range are clipped.
pub fn interleave_f32_to_i16(channels: &[&[f32]], out_channels: usize) -> Vec<i16> {
    let Some(frames) = channels.iter().map(|channel| channel.len()).min() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(frames * out_channels);
    for frame in 0..frames {
        for channel in 0..out_channels {
            let source = channels[channel.min(channels.len() - 1)];
            out.push(to_i16(source[frame]));
        }
    }
    out
}

/// Splits interleaved float samples into one slice per channel, for [`interleave_f32_to_i16`].
pub fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let channels = channels.max(1);
    (0..channels)
        .map(|channel| samples.iter().skip(channel).step_by(channels).copied().collect())
        .collect()
}

/// Converts float audio from one sample rate to another by linear interpolation, chunk by chunk, carrying its
/// position across chunks so that the joins are seamless. Good enough for a voice; not a mastering resampler.
#[derive(Debug)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output sample falls, in input samples from the start of the next chunk. It is below 0 when it
    /// falls between the previous chunk's last sample and the next chunk's first.
    next: f64,
    /// The previous chunk's last sample of each channel.
    last: Vec<f32>,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: f64::from(from_rate.max(1)) / f64::from(to_rate.max(1)),
            next: 0.0,
            last: Vec::new(),
        }
    }

    /// Resamples the next chunk, one slice per channel. Channels of different lengths are cut to the shortest.
    pub fn process(&mut self, channels: &[&[f32]]) -> Vec<Vec<f32>> {
        if self.is_identity() {
            return channels.iter().map(|channel| channel.to_vec()).collect();
        }
        let length = channels.iter().map(|channel| channel.len()).min().unwrap_or(0);
        if length == 0 {
            return vec![Vec::new(); channels.len()];
        }
        self.last.resize(channels.len(), 0.0);
        let mut out = vec![Vec::new(); channels.len()];
        let mut position = self.next;
        while position <= (length - 1) as f64 {
            let index = position.floor();
            let fraction = (position - index) as f32;
            for (channel, samples) in channels.iter().enumerate() {
                let at = |i: f64| {
                    if i < 0.0 {
                        self.last[channel]
                    } else {
                        samples[(i as usize).min(length - 1)]
                    }
                };
                let (a, b) = (at(index), at(index + 1.0));
                out[channel].push(a + (b - a) * fraction);
            }
            position += self.step;
        }
        self.next = position - length as f64;
        for (channel, samples) in channels.iter().enumerate() {
            self.last[channel] = samples[length - 1];
        }
        out
    }

    /// Resamples the next chunk and interleaves it into 16-bit samples, as [`interleave_f32_to_i16`] does. At the
    /// same rate the samples are converted directly, without an intermediate copy.
    pub fn resample_to_i16(&mut self, channels: &[&[f32]], out_channels: usize) -> Vec<i16> {
        if self.is_identity() {
            return interleave_f32_to_i16(channels, out_channels);
        }
        let resampled = self.process(channels);
        let resampled: Vec<&[f32]> = resampled.iter().map(Vec::as_slice).collect();
        interleave_f32_to_i16(&resampled, out_channels)
    }

    fn is_identity(&self) -> bool {
        (self.step - 1.0).abs() < f64::EPSILON
    }
}

/// What a sound capture delivered over a recording, for the log: how many buffers came, how many could not be used and
/// why, their format and the loudest sample. Without it a capture that delivers nothing, or only silence, cannot be
/// told apart from a quiet room.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SoundReport {
    buffers: u64,
    frames: u64,
    peak: f32,
    unusable: u64,
    first_problem: Option<String>,
    format: Option<String>,
}

impl SoundReport {
    /// Counts a buffer that was used, given one slice per channel.
    pub fn usable(&mut self, channels: &[&[f32]]) {
        self.buffers += 1;
        self.frames += channels.iter().map(|channel| channel.len()).min().unwrap_or(0) as u64;
        let peak = channels
            .iter()
            .flat_map(|channel| channel.iter())
            .filter(|sample| !sample.is_nan())
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        self.peak = self.peak.max(peak);
    }

    /// Counts a buffer that could not be used. The first reason is kept.
    pub fn unusable(&mut self, problem: impl Into<String>) {
        self.buffers += 1;
        self.unusable += 1;
        self.first_problem.get_or_insert_with(|| problem.into());
    }

    /// Notes the format the buffers arrive in. The first one is kept; `describe` is called only for it.
    pub fn format(&mut self, describe: impl FnOnce() -> String) {
        self.format.get_or_insert_with(describe);
    }
}

impl std::fmt::Display for SoundReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} buffers", self.buffers)?;
        if self.buffers == 0 {
            return Ok(());
        }
        write!(f, ", {} frames", self.frames)?;
        if self.peak > 0.0 {
            write!(f, ", loudest {:.1} dBFS", 20.0 * self.peak.log10())?;
        } else {
            write!(f, ", all silent")?;
        }
        if self.unusable > 0 {
            let problem = self.first_problem.as_deref().unwrap_or("no reason given");
            write!(f, ", {} not usable ({problem})", self.unusable)?;
        }
        if let Some(format) = &self.format {
            write!(f, ", format {format}")?;
        }
        Ok(())
    }
}

fn to_i16(sample: f32) -> i16 {
    // NaN becomes silence; everything else is clipped to the 16-bit range.
    if sample.is_nan() {
        return 0;
    }
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_channels_are_interleaved_and_scaled() {
        let left = [0.0, 1.0, -1.0];
        let right = [0.5, -0.5, 0.25];
        assert_eq!(
            interleave_f32_to_i16(&[&left, &right], 2),
            [0, 16384, 32767, -16384, -32767, 8192]
        );
    }

    #[test]
    fn a_mono_source_is_heard_on_both_sides() {
        assert_eq!(
            interleave_f32_to_i16(&[&[0.5, -0.5]], 2),
            [16384, 16384, -16384, -16384]
        );
    }

    #[test]
    fn extra_channels_are_cut_and_out_of_range_values_clipped() {
        let samples = interleave_f32_to_i16(&[&[2.0], &[-3.0], &[0.9]], 2);
        assert_eq!(samples, [32767, -32767]);
        assert_eq!(interleave_f32_to_i16(&[&[f32::NAN]], 1), [0]);
    }

    #[test]
    fn channels_of_different_lengths_use_the_shortest() {
        assert_eq!(interleave_f32_to_i16(&[&[0.0, 0.0, 0.0], &[0.0]], 2).len(), 2);
        assert!(interleave_f32_to_i16(&[], 2).is_empty());
    }

    #[test]
    fn the_same_rate_passes_through() {
        let mut resampler = Resampler::new(48_000, 48_000);
        assert_eq!(resampler.process(&[&[0.1, 0.2]]), [vec![0.1, 0.2]]);
    }

    #[test]
    fn doubling_the_rate_interpolates_between_samples() {
        let mut resampler = Resampler::new(24_000, 48_000);
        let out = resampler.process(&[&[0.0, 1.0, 0.0]]);
        assert_eq!(out, [vec![0.0, 0.5, 1.0, 0.5, 0.0]]);
    }

    #[test]
    fn chunks_join_without_a_seam() {
        let signal: Vec<f32> = (0..441).map(|i| (i as f32 * 0.05).sin()).collect();
        let whole = Resampler::new(44_100, 48_000).process(&[&signal]);

        let mut chunked = Resampler::new(44_100, 48_000);
        let mut pieces: Vec<f32> = Vec::new();
        for chunk in signal.chunks(100) {
            pieces.extend(&chunked.process(&[chunk])[0]);
        }
        assert_eq!(pieces.len(), whole[0].len());
        for (a, b) in pieces.iter().zip(&whole[0]) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
        // About 48000/44100 output samples per input sample.
        assert!((pieces.len() as f64 - 441.0 * 48_000.0 / 44_100.0).abs() <= 1.0);
    }

    #[test]
    fn resampling_to_i16_matches_resampling_then_interleaving() {
        let left = [0.0, 0.5, 1.0, 0.5];
        let right = [0.0, -0.5, -1.0, -0.5];
        let channels: [&[f32]; 2] = [&left, &right];

        assert_eq!(
            Resampler::new(48_000, 48_000).resample_to_i16(&channels, 2),
            interleave_f32_to_i16(&channels, 2)
        );
        let resampled = Resampler::new(24_000, 48_000).process(&channels);
        let resampled: Vec<&[f32]> = resampled.iter().map(Vec::as_slice).collect();
        assert_eq!(
            Resampler::new(24_000, 48_000).resample_to_i16(&channels, 2),
            interleave_f32_to_i16(&resampled, 2)
        );
    }

    #[test]
    fn a_sound_report_says_when_nothing_arrived() {
        assert_eq!(SoundReport::default().to_string(), "0 buffers");
    }

    #[test]
    fn a_sound_report_tells_silence_from_sound() {
        let mut silent = SoundReport::default();
        silent.usable(&[&[0.0, 0.0], &[0.0, 0.0]]);
        assert_eq!(silent.to_string(), "1 buffers, 2 frames, all silent");

        let mut sound = SoundReport::default();
        sound.format(|| "48000 Hz".into());
        sound.format(|| unreachable!("only the first format is described"));
        sound.usable(&[&[0.0, -0.5, f32::NAN], &[0.1, 0.25]]);
        sound.usable(&[&[0.1]]);
        assert_eq!(
            sound.to_string(),
            "2 buffers, 3 frames, loudest -6.0 dBFS, format 48000 Hz"
        );
    }

    #[test]
    fn a_sound_report_keeps_the_first_reason_a_buffer_was_not_usable() {
        let mut report = SoundReport::default();
        report.unusable("not float");
        report.unusable("other");
        report.usable(&[&[1.0]]);
        assert_eq!(
            report.to_string(),
            "3 buffers, 1 frames, loudest 0.0 dBFS, 2 not usable (not float)"
        );
    }

    #[test]
    fn interleaved_samples_split_back_into_channels() {
        let split = deinterleave(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2);
        assert_eq!(split, [vec![1.0, 3.0, 5.0], vec![2.0, 4.0, 6.0]]);
    }
}
