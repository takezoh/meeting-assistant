//! How one recording proceeds. It touches the OS only through a port ([`CaptureBackend`]).
//!
//! At a fixed interval, writes the latest picture and the audio for that interval. While the picture does not change,
//! the same picture keeps being written, so video and audio times always advance together.

use std::path::Path;
use std::time::Duration;

use meeting_assistant_core::{AudioConfig, RecordingRequest, VideoConfig};

use super::plan::{self, AudioSource, CHANNELS, MediaFormat, SAMPLE_RATE, VideoSource};
use super::timeline::Timeline;

/// Upper limit on waiting for the first picture after capture starts.
const FIRST_PICTURE_TIMEOUT: Duration = Duration::from_secs(2);
const FIRST_PICTURE_POLL: Duration = Duration::from_millis(10);
/// If writing falls behind by this much (e.g. after resuming from sleep),
/// jump to the current time instead of catching up.
const RESYNC_THRESHOLD: Duration = Duration::from_secs(2);

/// Capture and writing, handled by the OS.
pub trait CaptureBackend {
    /// The video target being captured.
    type Video;
    /// The audio input being captured.
    type Audio;
    /// One picture, fitted to the output size.
    type Picture;
    /// The output file.
    type Writer;

    /// Starts capturing video. Also returns the size of the target.
    fn open_video(&mut self, source: &VideoSource) -> anyhow::Result<(Self::Video, (u32, u32))>;
    /// Starts capturing audio.
    fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<Self::Audio>;
    /// Opens the output. From then on, pictures are handled at the size given by `format`.
    fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<Self::Writer>;

    /// If a new picture has arrived since the previous call, returns it fitted to the output size.
    /// Errors if the target is gone and capture cannot continue (a closed window is treated as having no new picture).
    fn take_picture(&mut self, video: &mut Self::Video) -> anyhow::Result<Option<Self::Picture>>;
    /// A solid black picture at the output size.
    fn black_picture(&mut self) -> anyhow::Result<Self::Picture>;
    /// Adds this input's sound to `out`.
    fn mix_audio(&mut self, audio: &mut Self::Audio, out: &mut [i16]);

    /// Time and duration are in 100 ns units.
    fn write_video(
        &mut self,
        writer: &mut Self::Writer,
        picture: &Self::Picture,
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()>;
    fn write_audio(&mut self, writer: &mut Self::Writer, pcm: &[i16], time: i64, duration: i64) -> anyhow::Result<()>;
    /// Writes out the rest and finalizes the file.
    fn finalize(&mut self, writer: Self::Writer) -> anyhow::Result<()>;
}

/// The passage of time, and waiting.
pub trait Pacer {
    /// Time elapsed since a certain point.
    fn elapsed(&self) -> Duration;
    fn sleep(&mut self, duration: Duration);
}

/// Instructions from outside the recording.
pub trait Control {
    fn should_stop(&self) -> bool;
    /// If there is an instruction to switch the video target, the window to switch to.
    fn take_retarget(&mut self) -> Option<u64>;
}

pub struct Pipeline<B: CaptureBackend> {
    // Release the capture and output first, then do the OS-side cleanup (fields are dropped in declaration order).
    video: Option<B::Video>,
    audio: Vec<B::Audio>,
    writer: B::Writer,
    picture: B::Picture,
    backend: B,
    framerate: u32,
    mix: Vec<i16>,
}

impl<B: CaptureBackend> Pipeline<B> {
    /// Prepares capture and writing. Returns once recording is ready.
    pub fn open(
        mut backend: B,
        request: &RecordingRequest,
        video_config: &VideoConfig,
        audio_config: &AudioConfig,
        pacer: &mut dyn Pacer,
    ) -> anyhow::Result<Self> {
        let source = plan::video_source(&request.video)?;
        let mut video = None;
        let mut source_size = None;
        if let Some(source) = &source {
            let (opened, size) = backend.open_video(source)?;
            video = Some(opened);
            source_size = Some(size);
        }
        let (width, height) = plan::canvas_size(source_size);

        // Even if an audio input is unavailable, keep recording with the rest.
        let mut audio = Vec::new();
        let mut labels = Vec::new();
        for audio_source in plan::audio_sources(request, audio_config) {
            match backend.open_audio(&audio_source) {
                Ok(opened) => {
                    audio.push(opened);
                    labels.push(audio_source.label());
                }
                Err(error) => log::warn!("{} will not be recorded: {error:#}", audio_source.label()),
            }
        }

        let format = MediaFormat {
            width,
            height,
            framerate: video_config.framerate,
            quality: video_config.quality,
            fallback_bitrate: video_config.fallback_bitrate_kbps * 1000,
            has_audio: !audio.is_empty(),
        };
        let writer = backend.open_writer(&request.output_path, &format)?;

        let picture = match (&mut video, &source) {
            (Some(video), Some(source)) => first_picture(&mut backend, video, source, pacer)?,
            _ => backend.black_picture()?,
        };

        log::info!(
            "recording is ready: {} {width}x{height} {}fps audio [{}]",
            match &source {
                Some(VideoSource::Window(_)) => "window",
                Some(VideoSource::Display(_)) => "display",
                None => "no video target",
            },
            video_config.framerate,
            labels.join(", "),
        );
        Ok(Self {
            video,
            audio,
            writer,
            picture,
            backend,
            framerate: video_config.framerate,
            mix: Vec::new(),
        })
    }

    /// Writes until told to stop, then finalizes the file.
    ///
    /// A failure while writing ends the recording, and the file is still finalized so that it keeps what was written.
    /// The writing failure is the one returned.
    pub fn run(mut self, control: &mut dyn Control, pacer: &mut dyn Pacer) -> anyhow::Result<()> {
        let written = self.write_until_stopped(control, pacer);

        // Stop capture first, then finalize the file.
        let Self {
            video,
            audio,
            writer,
            mut backend,
            ..
        } = self;
        drop(audio);
        let finalized = backend.finalize(writer);
        drop(video);
        if let (Err(_), Err(error)) = (&written, &finalized) {
            log::warn!("Could not finalize the recording after it failed: {error:#}");
        }
        written.and(finalized)
    }

    fn write_until_stopped(&mut self, control: &mut dyn Control, pacer: &mut dyn Pacer) -> anyhow::Result<()> {
        let started = pacer.elapsed();
        let mut timeline = Timeline::new(self.framerate, SAMPLE_RATE);

        while !control.should_stop() {
            let elapsed = pacer.elapsed().saturating_sub(started);
            let wait = timeline.next_due().saturating_sub(elapsed);
            if wait.is_zero() {
                timeline.skip_ahead_if_late(elapsed, RESYNC_THRESHOLD);
            } else {
                pacer.sleep(wait);
            }

            while let Some(window) = control.take_retarget() {
                self.switch_window(window);
            }
            if let Some(video) = &mut self.video
                && let Some(picture) = self.backend.take_picture(video)?
            {
                self.picture = picture;
            }

            let step = timeline.step();
            self.backend
                .write_video(&mut self.writer, &self.picture, step.video_time, step.video_duration)?;
            if !self.audio.is_empty() {
                self.mix.clear();
                self.mix.resize(step.audio_frames * CHANNELS, 0);
                for audio in &mut self.audio {
                    self.backend.mix_audio(audio, &mut self.mix);
                }
                self.backend
                    .write_audio(&mut self.writer, &self.mix, step.audio_time, step.audio_duration)?;
            }
        }
        Ok(())
    }

    /// Moves the video target to another window. If that fails, continues with the previous video.
    fn switch_window(&mut self, window: u64) {
        match self.backend.open_video(&VideoSource::Window(window)) {
            Ok((video, _)) => {
                self.video = Some(video);
                log::info!("Switched the window being recorded.");
            }
            Err(error) => log::warn!("Could not switch the window being recorded: {error:#}"),
        }
    }
}

/// The first picture, right after capture starts.
/// A minimized window delivers no pictures, so when the target is a window we start with a black picture.
fn first_picture<B: CaptureBackend>(
    backend: &mut B,
    video: &mut B::Video,
    source: &VideoSource,
    pacer: &mut dyn Pacer,
) -> anyhow::Result<B::Picture> {
    let deadline = pacer.elapsed() + FIRST_PICTURE_TIMEOUT;
    loop {
        if let Some(picture) = backend.take_picture(video)? {
            return Ok(picture);
        }
        if pacer.elapsed() >= deadline {
            anyhow::ensure!(source.is_window(), "cannot obtain the display's picture");
            return backend.black_picture();
        }
        pacer.sleep(FIRST_PICTURE_POLL);
    }
}

#[cfg(test)]
pub mod fakes {
    //! Fake capture and writing. The arriving pictures are scripted, and what gets written is recorded.

    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, MutexGuard};

    use super::*;

    /// One written item.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Written {
        Video {
            picture: String,
            time: i64,
            duration: i64,
        },
        Audio {
            samples: Vec<i16>,
            time: i64,
            duration: i64,
        },
    }

    #[derive(Default)]
    pub struct BackendState {
        /// The video targets that were opened.
        pub opened_video: Vec<VideoSource>,
        pub opened_audio: Vec<AudioSource>,
        pub writer_path: Option<PathBuf>,
        pub format: Option<MediaFormat>,
        pub written: Vec<Written>,
        pub finalized: bool,
        /// Size per target (1280x720 if not specified).
        pub video_size: (u32, u32),
        /// Pictures that will arrive. Used from the front, one per `take_picture` call
        /// (`None` means "nothing arrived").
        pub pictures: VecDeque<Option<String>>,
        /// Video target for which opening fails.
        pub failing_video: Vec<VideoSource>,
        pub failing_audio: Vec<AudioSource>,
        /// While true, video capture fails.
        pub video_broken: bool,
        /// Sound value to mix, per input.
        pub audio_level: i16,
    }

    #[derive(Clone)]
    pub struct FakeBackend(Arc<Mutex<BackendState>>);

    impl Default for FakeBackend {
        fn default() -> Self {
            Self(Arc::new(Mutex::new(BackendState {
                video_size: (1280, 720),
                audio_level: 1,
                ..Default::default()
            })))
        }
    }

    impl FakeBackend {
        pub fn state(&self) -> MutexGuard<'_, BackendState> {
            self.0.lock().unwrap()
        }

        /// Names of the pictures that were written.
        pub fn written_pictures(&self) -> Vec<String> {
            let state = self.state();
            state
                .written
                .iter()
                .filter_map(|w| {
                    if let Written::Video { picture, .. } = w {
                        Some(picture.clone())
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    impl CaptureBackend for FakeBackend {
        type Video = VideoSource;
        type Audio = AudioSource;
        type Picture = String;
        type Writer = ();

        fn open_video(&mut self, source: &VideoSource) -> anyhow::Result<(VideoSource, (u32, u32))> {
            let mut state = self.state();
            anyhow::ensure!(!state.failing_video.contains(source), "cannot capture {source:?}");
            state.opened_video.push(source.clone());
            Ok((source.clone(), state.video_size))
        }

        fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<AudioSource> {
            let mut state = self.state();
            anyhow::ensure!(!state.failing_audio.contains(source), "cannot capture {source:?}");
            state.opened_audio.push(source.clone());
            Ok(source.clone())
        }

        fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<()> {
            let mut state = self.state();
            state.writer_path = Some(path.to_owned());
            state.format = Some(format.clone());
            Ok(())
        }

        fn take_picture(&mut self, _video: &mut VideoSource) -> anyhow::Result<Option<String>> {
            let mut state = self.state();
            anyhow::ensure!(!state.video_broken, "display lost");
            Ok(state.pictures.pop_front().flatten())
        }

        fn black_picture(&mut self) -> anyhow::Result<String> {
            Ok("black".to_owned())
        }

        fn mix_audio(&mut self, _audio: &mut AudioSource, out: &mut [i16]) {
            let level = self.state().audio_level;
            for sample in out {
                *sample += level;
            }
        }

        fn write_video(&mut self, _: &mut (), picture: &String, time: i64, duration: i64) -> anyhow::Result<()> {
            self.state().written.push(Written::Video {
                picture: picture.clone(),
                time,
                duration,
            });
            Ok(())
        }

        fn write_audio(&mut self, _: &mut (), pcm: &[i16], time: i64, duration: i64) -> anyhow::Result<()> {
            self.state().written.push(Written::Audio {
                samples: pcm.to_vec(),
                time,
                duration,
            });
            Ok(())
        }

        fn finalize(&mut self, _: ()) -> anyhow::Result<()> {
            self.state().finalized = true;
            Ok(())
        }
    }

    /// Advances the clock instead of waiting.
    #[derive(Default)]
    pub struct FakePacer {
        pub now: Duration,
        pub slept: Vec<Duration>,
    }

    impl Pacer for FakePacer {
        fn elapsed(&self) -> Duration {
            self.now
        }

        fn sleep(&mut self, duration: Duration) {
            self.now += duration;
            self.slept.push(duration);
        }
    }

    /// Lets it write a fixed number of times, then stops. A video target switch can be requested midway.
    pub struct StopAfter {
        pub remaining: std::cell::Cell<u32>,
        /// (the remaining count at which to switch, the window to switch to)
        pub retargets: Vec<(u32, u64)>,
    }

    impl StopAfter {
        pub fn steps(steps: u32) -> Self {
            Self {
                remaining: std::cell::Cell::new(steps),
                retargets: Vec::new(),
            }
        }
    }

    impl Control for StopAfter {
        fn should_stop(&self) -> bool {
            if self.remaining.get() == 0 {
                return true;
            }
            self.remaining.set(self.remaining.get() - 1);
            false
        }

        fn take_retarget(&mut self) -> Option<u64> {
            let remaining = self.remaining.get();
            let index = self.retargets.iter().position(|(at, _)| *at == remaining)?;
            Some(self.retargets.remove(index).1)
        }
    }
}

#[cfg(test)]
mod tests {
    use meeting_assistant_core::VideoTarget;

    use super::fakes::*;
    use super::*;

    fn request(video: VideoTarget, audio_process_id: Option<u32>) -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video,
            audio_process_id,
            render_device_id: None,
            capture_device_id: None,
        }
    }

    fn video_config() -> VideoConfig {
        VideoConfig {
            framerate: 30,
            quality: 70,
            fallback_bitrate_kbps: 4000,
        }
    }

    fn open(backend: &FakeBackend, video: VideoTarget) -> anyhow::Result<Pipeline<FakeBackend>> {
        Pipeline::open(
            backend.clone(),
            &request(video, Some(100)),
            &video_config(),
            &AudioConfig::default(),
            &mut FakePacer::default(),
        )
    }

    /// Opens window 7 with its first picture "first" already arrived.
    fn opened_window() -> (FakeBackend, Pipeline<FakeBackend>) {
        let backend = FakeBackend::default();
        backend.state().pictures.push_back(Some("first".into()));
        let pipeline = open(&backend, VideoTarget::Window(7)).unwrap();
        (backend, pipeline)
    }

    #[test]
    fn captures_the_requested_target_and_opens_the_output_in_the_configured_format() {
        let (backend, _pipeline) = opened_window();

        let state = backend.state();
        assert_eq!(state.opened_video, [VideoSource::Window(7)]);
        assert_eq!(
            state.opened_audio,
            [AudioSource::ProcessAudio(100), AudioSource::Microphone(None)]
        );
        assert_eq!(state.writer_path.as_deref(), Some(Path::new("out.mp4")));
        assert_eq!(
            state.format,
            Some(MediaFormat {
                width: 1280,
                height: 720,
                framerate: 30,
                quality: 70,
                fallback_bitrate: 4_000_000,
                has_audio: true
            })
        );
    }

    #[test]
    fn output_size_is_the_target_size_made_even() {
        let backend = FakeBackend::default();
        backend.state().video_size = (1585, 993);
        backend.state().pictures.push_back(Some("first".into()));
        open(&backend, VideoTarget::Window(7)).unwrap();

        let format = backend.state().format.clone().unwrap();
        assert_eq!((format.width, format.height), (1584, 992));
    }

    #[test]
    fn records_with_the_remaining_inputs_when_an_audio_input_cannot_be_captured() {
        let backend = FakeBackend::default();
        backend.state().failing_audio = vec![AudioSource::ProcessAudio(100)];
        backend.state().pictures.push_back(Some("first".into()));
        open(&backend, VideoTarget::Window(7)).unwrap();

        let state = backend.state();
        assert_eq!(
            state.opened_audio,
            [AudioSource::Microphone(None)],
            "output device audio is not substituted"
        );
        assert!(state.format.as_ref().unwrap().has_audio);
    }

    #[test]
    fn records_video_only_when_no_audio_input_is_available() {
        let backend = FakeBackend::default();
        backend.state().failing_audio = vec![AudioSource::ProcessAudio(100), AudioSource::Microphone(None)];
        backend.state().pictures.push_back(Some("first".into()));
        let pipeline = open(&backend, VideoTarget::Window(7)).unwrap();
        assert!(!backend.state().format.as_ref().unwrap().has_audio);

        pipeline
            .run(&mut StopAfter::steps(3), &mut FakePacer::default())
            .unwrap();

        assert!(
            backend
                .state()
                .written
                .iter()
                .all(|w| matches!(w, Written::Video { .. }))
        );
    }

    #[test]
    fn does_not_start_recording_when_the_video_target_cannot_be_opened() {
        let backend = FakeBackend::default();
        backend.state().failing_video = vec![VideoSource::Window(7)];

        assert!(open(&backend, VideoTarget::Window(7)).is_err());
        assert!(backend.state().writer_path.is_none(), "the output is not created");
    }

    #[test]
    fn starts_with_a_black_picture_when_there_is_no_video_target() {
        let backend = FakeBackend::default();
        let pipeline = open(&backend, VideoTarget::Blank).unwrap();
        assert!(backend.state().opened_video.is_empty());

        pipeline
            .run(&mut StopAfter::steps(2), &mut FakePacer::default())
            .unwrap();

        assert_eq!(backend.written_pictures(), ["black", "black"]);
    }

    #[test]
    fn starts_with_a_black_picture_when_the_window_delivers_no_picture() {
        let backend = FakeBackend::default();
        let mut pacer = FakePacer::default();
        let pipeline = Pipeline::open(
            backend.clone(),
            &request(VideoTarget::Window(7), None),
            &video_config(),
            &AudioConfig::default(),
            &mut pacer,
        )
        .unwrap();
        assert!(
            pacer.now >= FIRST_PICTURE_TIMEOUT,
            "wait for it to arrive before giving up"
        );

        pipeline
            .run(&mut StopAfter::steps(1), &mut FakePacer::default())
            .unwrap();
        assert_eq!(backend.written_pictures(), ["black"]);
    }

    #[test]
    fn does_not_start_recording_when_the_display_delivers_no_picture() {
        let backend = FakeBackend::default();

        assert!(open(&backend, VideoTarget::Display(None)).is_err());
    }

    #[test]
    fn uses_a_first_picture_that_arrives_slightly_late() {
        let backend = FakeBackend::default();
        backend.state().pictures.extend([None, None, Some("late".into())]);
        let pipeline = open(&backend, VideoTarget::Display(None)).unwrap();

        pipeline
            .run(&mut StopAfter::steps(1), &mut FakePacer::default())
            .unwrap();
        assert_eq!(backend.written_pictures(), ["late"]);
    }

    #[test]
    fn keeps_writing_the_same_picture_until_a_new_one_arrives_then_switches() {
        let (backend, pipeline) = opened_window();
        backend.state().pictures.extend([None, Some("second".into()), None]);

        pipeline
            .run(&mut StopAfter::steps(4), &mut FakePacer::default())
            .unwrap();

        assert_eq!(backend.written_pictures(), ["first", "second", "second", "second"]);
    }

    #[test]
    fn writes_video_and_audio_alternately_at_the_same_time() {
        let (backend, pipeline) = opened_window();
        pipeline
            .run(&mut StopAfter::steps(2), &mut FakePacer::default())
            .unwrap();

        let state = backend.state();
        let times: Vec<(bool, i64, i64)> = state
            .written
            .iter()
            .map(|w| match w {
                Written::Video { time, duration, .. } => (true, *time, *duration),
                Written::Audio { time, duration, .. } => (false, *time, *duration),
            })
            .collect();
        assert_eq!(
            times,
            [
                (true, 0, 333_333),
                (false, 0, 333_333),
                (true, 333_333, 333_333),
                (false, 333_333, 333_333)
            ]
        );
    }

    #[test]
    fn audio_adds_all_inputs_and_is_written_one_picture_at_a_time() {
        let (backend, pipeline) = opened_window();
        backend.state().audio_level = 3;
        pipeline
            .run(&mut StopAfter::steps(1), &mut FakePacer::default())
            .unwrap();

        let state = backend.state();
        let Some(Written::Audio { samples, .. }) = state.written.iter().find(|w| matches!(w, Written::Audio { .. }))
        else {
            panic!("audio was not written");
        };
        assert_eq!(samples.len(), 1_600 * CHANNELS, "one picture at 30fps");
        assert!(
            samples.iter().all(|&sample| sample == 6),
            "the sound of the two inputs is added"
        );
    }

    #[test]
    fn writes_at_one_picture_intervals() {
        let (_backend, pipeline) = opened_window();
        let mut pacer = FakePacer::default();

        pipeline.run(&mut StopAfter::steps(31), &mut pacer).unwrap();

        assert_eq!(
            pacer.now,
            Duration::from_secs(1),
            "1 second by the time the 31st picture is written"
        );
        assert_eq!(pacer.slept.len(), 30, "the first picture is written without waiting");
    }

    #[test]
    fn writes_with_times_counted_from_the_start_of_recording() {
        let (backend, pipeline) = opened_window();
        let mut pacer = FakePacer {
            now: Duration::from_secs(1000),
            ..Default::default()
        };

        pipeline.run(&mut StopAfter::steps(1), &mut pacer).unwrap();

        assert!(
            matches!(backend.state().written[0], Written::Video { time: 0, .. }),
            "time spent on preparation is not included in the timestamps"
        );
        assert!(pacer.slept.is_empty());
    }

    #[test]
    fn finalizes_the_file_on_the_stop_instruction() {
        let (backend, pipeline) = opened_window();
        pipeline
            .run(&mut StopAfter::steps(0), &mut FakePacer::default())
            .unwrap();

        assert!(backend.state().finalized);
        assert!(backend.state().written.is_empty());
    }

    #[test]
    fn ends_as_a_failure_when_video_capture_can_no_longer_continue_and_still_finalizes() {
        let (backend, pipeline) = opened_window();
        backend.state().video_broken = true;

        let error = pipeline
            .run(&mut StopAfter::steps(5), &mut FakePacer::default())
            .unwrap_err();
        assert!(error.to_string().contains("display lost"));
        assert!(
            backend.state().finalized,
            "the file keeps what was written before the failure"
        );
    }

    #[test]
    fn moves_the_video_target_to_another_window_when_instructed() {
        let (backend, pipeline) = opened_window();
        backend
            .state()
            .pictures
            .extend([None, None, Some("from new window".into())]);
        // Request the switch before the second write.
        let mut control = StopAfter {
            retargets: vec![(1, 8)],
            ..StopAfter::steps(3)
        };

        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        assert_eq!(
            backend.state().opened_video,
            [VideoSource::Window(7), VideoSource::Window(8)]
        );
        assert_eq!(backend.written_pictures(), ["first", "first", "from new window"]);
        assert_eq!(
            backend.state().format.as_ref().map(|f| (f.width, f.height)),
            Some((1280, 720)),
            "the size does not change"
        );
    }

    #[test]
    fn continues_with_the_previous_video_when_the_destination_window_cannot_be_opened() {
        let (backend, pipeline) = opened_window();
        backend.state().failing_video = vec![VideoSource::Window(8)];
        let mut control = StopAfter {
            retargets: vec![(1, 8)],
            ..StopAfter::steps(2)
        };

        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        assert_eq!(backend.written_pictures(), ["first", "first"]);
        assert!(backend.state().finalized);
    }

    #[test]
    fn a_window_can_be_given_to_a_recording_started_without_a_target() {
        let backend = FakeBackend::default();
        let pipeline = open(&backend, VideoTarget::Blank).unwrap();
        backend.state().pictures.push_back(Some("window".into()));
        // Give a window before the second write.
        let mut control = StopAfter {
            retargets: vec![(0, 8)],
            ..StopAfter::steps(2)
        };

        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        assert_eq!(backend.written_pictures(), ["black", "window"]);
    }
}
