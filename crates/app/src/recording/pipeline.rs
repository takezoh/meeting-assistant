//! How one recording proceeds. It touches the OS only through a port ([`CaptureBackend`]).
//!
//! At a fixed interval, writes the latest picture and, for each audio track, the sound of that interval. While the
//! picture does not change, the same picture keeps being written, so video and audio times always advance together.
//! Each audio track has its own source and is written on its own; a track without a source is written as silence.

use std::path::Path;
use std::time::Duration;

use meeting_assistant_core::VideoConfig;

use super::plan::{AudioSource, CHANNELS, CapturePlan, MediaFormat, SAMPLE_RATE, VideoPlan, VideoSource};
use super::timeline::{Step, Timeline};

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

    /// Starts capturing video, with each picture fitted into `canvas` (width, height) whatever the target's size and
    /// shape, its aspect ratio kept.
    fn open_video(&mut self, source: &VideoSource, canvas: (u32, u32)) -> anyhow::Result<Self::Video>;
    /// Starts capturing audio.
    fn open_audio(&mut self, source: &AudioSource) -> anyhow::Result<Self::Audio>;
    /// Opens the output. From then on, pictures are handled at the size given by `format`.
    fn open_writer(&mut self, path: &Path, format: &MediaFormat) -> anyhow::Result<Self::Writer>;

    /// If a new picture has arrived since the previous call, returns it fitted to the output size.
    /// Errors if the target is gone and capture cannot continue (a closed window is treated as having no new picture).
    fn take_picture(&mut self, video: &mut Self::Video) -> anyhow::Result<Option<Self::Picture>>;
    /// A solid black picture at the output size.
    fn black_picture(&mut self) -> anyhow::Result<Self::Picture>;
    /// Writing starts now. Drops the sound this input captured before, apart from the little kept against jitter,
    /// so that its first sound written lines up with the first picture.
    fn start_writing(&mut self, audio: &mut Self::Audio);
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
    /// Writes sound to the audio track at `track`, an index into [`MediaFormat::audio_tracks`].
    fn write_audio(
        &mut self,
        writer: &mut Self::Writer,
        track: usize,
        pcm: &[i16],
        time: i64,
        duration: i64,
    ) -> anyhow::Result<()>;
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
    /// One entry per audio track, in the file's order; `None` for a track written as silence.
    audio: Vec<Option<B::Audio>>,
    writer: B::Writer,
    /// The picture being written; `None` when the file has no video track.
    picture: Option<B::Picture>,
    backend: B,
    framerate: u32,
    /// The size every picture is fitted into.
    canvas: (u32, u32),
    block: Vec<i16>,
}

impl<B: CaptureBackend> Pipeline<B> {
    /// Prepares capture and writing to `output_path`. Returns once recording is ready.
    pub fn open(
        mut backend: B,
        plan: &CapturePlan,
        output_path: &Path,
        video_config: &VideoConfig,
        pacer: &mut dyn Pacer,
    ) -> anyhow::Result<Self> {
        let canvas = (video_config.width, video_config.height);
        let mut video = match &plan.video {
            VideoPlan::Capture(source) => Some(backend.open_video(source, canvas)?),
            VideoPlan::Blank | VideoPlan::None => None,
        };
        let (width, height) = canvas;

        // A source that cannot be opened leaves its track silent; nothing else takes its place.
        let mut audio = Vec::new();
        let mut labels = Vec::new();
        for (track, source) in &plan.tracks {
            let opened = source.as_ref().and_then(|source| match backend.open_audio(source) {
                Ok(opened) => Some((opened, source.label())),
                Err(error) => {
                    log::warn!(
                        "{} will not be recorded; its track stays silent: {error:#}",
                        source.label()
                    );
                    None
                }
            });
            let label = opened
                .as_ref()
                .map_or_else(|| "silence".to_owned(), |(_, label)| label.clone());
            labels.push(format!("{}: {label}", track.label()));
            audio.push(opened.map(|(opened, _)| opened));
        }

        let has_video = plan.video != VideoPlan::None;
        let format = MediaFormat {
            width,
            height,
            framerate: video_config.framerate,
            quality: video_config.quality,
            fallback_bitrate: video_config.fallback_bitrate_kbps * 1000,
            has_video,
            audio_tracks: plan.tracks.iter().map(|(track, _)| *track).collect(),
        };
        let writer = backend.open_writer(output_path, &format)?;

        let picture = match (&mut video, &plan.video) {
            (Some(video), VideoPlan::Capture(source)) => Some(first_picture(&mut backend, video, source, pacer)?),
            _ if has_video => Some(backend.black_picture()?),
            _ => None,
        };

        let video_label = match &plan.video {
            VideoPlan::Capture(VideoSource::Window(_)) => {
                format!("window {width}x{height} {}fps", video_config.framerate)
            }
            VideoPlan::Capture(VideoSource::Display(_)) => {
                format!("display {width}x{height} {}fps", video_config.framerate)
            }
            VideoPlan::Blank => format!("no video target {width}x{height} {}fps", video_config.framerate),
            VideoPlan::None => "no video".to_owned(),
        };
        log::info!("recording is ready: {video_label} audio [{}]", labels.join(", "));
        Ok(Self {
            video,
            audio,
            writer,
            picture,
            backend,
            framerate: video_config.framerate,
            canvas,
            block: Vec::new(),
        })
    }

    /// Writes until told to stop, then finalizes the file.
    ///
    /// A failure while writing ends the recording, and the file is still finalized so that it keeps what was written.
    /// The writing failure is the one returned.
    pub fn run(mut self, control: &mut dyn Control, pacer: &mut dyn Pacer) -> anyhow::Result<()> {
        let mut pacing = Pacing::default();
        let written = self.write_until_stopped(control, pacer, &mut pacing);
        pacing.log(self.framerate);

        // Stop capture first, then finalize the file, and release everything before the OS-side cleanup in `backend`.
        let Self {
            video,
            audio,
            writer,
            picture,
            mut backend,
            ..
        } = self;
        drop(audio);
        let finalized = backend.finalize(writer);
        drop(picture);
        drop(video);
        if let (Err(_), Err(error)) = (&written, &finalized) {
            log::warn!("Could not finalize the recording after it failed: {error:#}");
        }
        written.and(finalized)
    }

    fn write_until_stopped(
        &mut self,
        control: &mut dyn Control,
        pacer: &mut dyn Pacer,
        pacing: &mut Pacing,
    ) -> anyhow::Result<()> {
        let started = pacer.elapsed();
        let mut timeline = Timeline::new(self.framerate, SAMPLE_RATE);
        let frame = Duration::from_secs(1) / self.framerate.max(1);
        // Audio was captured while the output and the first picture were being prepared; it would lag the picture by
        // that long for the whole recording.
        for audio in self.audio.iter_mut().flatten() {
            self.backend.start_writing(audio);
        }
        // The sound of a frame interval is written at the next step, once it has been captured. Mixed at the start
        // of its own interval it would have to come from what was captured before, a frame late.
        let mut captured_next: Option<Step> = None;

        while !control.should_stop() {
            let elapsed = pacer.elapsed().saturating_sub(started);
            let wait = timeline.next_due().saturating_sub(elapsed);
            if wait.is_zero() {
                let behind = elapsed.saturating_sub(timeline.next_due());
                if timeline.skip_ahead_if_late(elapsed, RESYNC_THRESHOLD) {
                    log::warn!(
                        "Writing fell {:.1} s behind; nothing was recorded for that time.",
                        behind.as_secs_f64()
                    );
                    pacing.skips += 1;
                    pacing.skipped += behind;
                    // The sound queued meanwhile belongs to the time jumped over; written from now on, it would lag
                    // the picture. The interval left from before the jump goes too.
                    for audio in self.audio.iter_mut().flatten() {
                        self.backend.start_writing(audio);
                    }
                    captured_next = None;
                } else if behind > frame {
                    pacing.late += 1;
                }
            } else {
                pacer.sleep(wait);
            }
            let began = pacer.elapsed();

            // Opening a capture takes time, so of several queued requests only the last one is opened.
            let mut retarget = None;
            while let Some(window) = control.take_retarget() {
                retarget = Some(window);
            }
            if let Some(window) = retarget {
                self.switch_window(window);
            }
            if let Some(video) = &mut self.video
                && let Some(picture) = self.backend.take_picture(video)?
            {
                self.picture = Some(picture);
            }

            let step = timeline.step();
            if let Some(picture) = &self.picture {
                self.backend
                    .write_video(&mut self.writer, picture, step.video_time, step.video_duration)?;
            }
            if let Some(previous) = captured_next.replace(step) {
                self.write_audio(&previous)?;
            }
            let busy = pacer.elapsed().saturating_sub(began);
            pacing.frames += 1;
            pacing.busy += busy;
            pacing.slowest = pacing.slowest.max(busy);
        }
        // The last interval, with as much of its sound as has arrived.
        if let Some(last) = captured_next {
            self.write_audio(&last)?;
        }
        Ok(())
    }

    /// Writes one interval's sound to each audio track: its own source's, or silence.
    fn write_audio(&mut self, step: &Step) -> anyhow::Result<()> {
        for (track, audio) in self.audio.iter_mut().enumerate() {
            self.block.clear();
            self.block.resize(step.audio_frames * CHANNELS, 0);
            if let Some(audio) = audio {
                self.backend.mix_audio(audio, &mut self.block);
            }
            self.backend.write_audio(
                &mut self.writer,
                track,
                &self.block,
                step.audio_time,
                step.audio_duration,
            )?;
        }
        Ok(())
    }

    /// Moves the video target to another window. If that fails, continues with the previous video. A recording without
    /// a video track has no window to move to.
    fn switch_window(&mut self, window: u64) {
        if self.picture.is_none() {
            return;
        }
        match self.backend.open_video(&VideoSource::Window(window), self.canvas) {
            Ok(video) => {
                self.video = Some(video);
                log::info!("Switched the window being recorded.");
            }
            Err(error) => log::warn!("Could not switch the window being recorded: {error:#}"),
        }
    }
}

/// How the write loop kept to its schedule, for the log.
#[derive(Default)]
struct Pacing {
    frames: u64,
    /// Frames that began more than one frame interval after they were due.
    late: u64,
    /// Times writing fell so far behind that it jumped to the current time, and the time it jumped over.
    skips: u32,
    skipped: Duration,
    /// The total and the longest time taking, writing and mixing one frame took.
    busy: Duration,
    slowest: Duration,
}

impl Pacing {
    fn log(&self, framerate: u32) {
        log::info!(
            "wrote {} frames at {framerate}fps: {} began more than a frame late, each took {:.2} ms on average \
             and the slowest {} ms, skipped ahead {} times ({:.1} s)",
            self.frames,
            self.late,
            self.busy.as_secs_f64() * 1000.0 / self.frames.max(1) as f64,
            self.slowest.as_millis(),
            self.skips,
            self.skipped.as_secs_f64(),
        );
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
            track: usize,
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
        /// Pictures that will arrive. Used from the front, one per `take_picture` call
        /// (`None` means "nothing arrived").
        pub pictures: VecDeque<Option<String>>,
        /// Video target for which opening fails.
        pub failing_video: Vec<VideoSource>,
        pub failing_audio: Vec<AudioSource>,
        /// While true, video capture fails.
        pub video_broken: bool,
        /// The canvas each video target was opened with.
        pub canvases: Vec<(u32, u32)>,
        /// Sound value an app or output device input adds.
        pub audio_level: i16,
        /// Sound value a microphone input adds, so that tests can tell the tracks apart.
        pub microphone_level: i16,
        /// The inputs told that writing starts, with how many items had been written at that moment.
        pub started_writing: Vec<(AudioSource, usize)>,
    }

    #[derive(Clone)]
    pub struct FakeBackend(Arc<Mutex<BackendState>>);

    impl Default for FakeBackend {
        fn default() -> Self {
            Self(Arc::new(Mutex::new(BackendState {
                audio_level: 1,
                microphone_level: 20,
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

        /// The sound written to each audio track, one block per write.
        pub fn written_audio(&self, track: usize) -> Vec<Vec<i16>> {
            self.state()
                .written
                .iter()
                .filter_map(|w| match w {
                    Written::Audio { track: t, samples, .. } if *t == track => Some(samples.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    impl CaptureBackend for FakeBackend {
        type Video = VideoSource;
        type Audio = AudioSource;
        type Picture = String;
        type Writer = ();

        fn open_video(&mut self, source: &VideoSource, canvas: (u32, u32)) -> anyhow::Result<VideoSource> {
            let mut state = self.state();
            state.canvases.push(canvas);
            anyhow::ensure!(!state.failing_video.contains(source), "cannot capture {source:?}");
            state.opened_video.push(source.clone());
            Ok(source.clone())
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

        fn start_writing(&mut self, audio: &mut AudioSource) {
            let mut state = self.state();
            let written = state.written.len();
            state.started_writing.push((audio.clone(), written));
        }

        fn mix_audio(&mut self, audio: &mut AudioSource, out: &mut [i16]) {
            let state = self.state();
            let level = match audio {
                AudioSource::Microphone(_) => state.microphone_level,
                _ => state.audio_level,
            };
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

        fn write_audio(
            &mut self,
            _: &mut (),
            track: usize,
            pcm: &[i16],
            time: i64,
            duration: i64,
        ) -> anyhow::Result<()> {
            self.state().written.push(Written::Audio {
                track,
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
    use meeting_assistant_core::{AudioConfig, RecordingRequest, VideoTarget};

    use super::super::plan::{self, Track};
    use super::fakes::*;
    use super::*;

    fn request(video: VideoTarget, audio_process_id: Option<u32>) -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video,
            audio_process_id,
            render_device_id: None,
            capture_device_id: None,
            microphone: true,
            metadata: meeting_assistant_core::RecordingMetadata::default(),
        }
    }

    fn open_request(
        backend: &FakeBackend,
        request: &RecordingRequest,
        pacer: &mut FakePacer,
    ) -> anyhow::Result<Pipeline<FakeBackend>> {
        let plan = plan::meeting_plan(request, &AudioConfig::default())?;
        Pipeline::open(backend.clone(), &plan, &request.output_path, &video_config(), pacer)
    }

    fn video_config() -> VideoConfig {
        VideoConfig {
            framerate: 30,
            quality: 70,
            fallback_bitrate_kbps: 4000,
            ..VideoConfig::default()
        }
    }

    fn open(backend: &FakeBackend, video: VideoTarget) -> anyhow::Result<Pipeline<FakeBackend>> {
        open_request(backend, &request(video, Some(100)), &mut FakePacer::default())
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
                width: 1920,
                height: 1080,
                framerate: 30,
                quality: 70,
                fallback_bitrate: 4_000_000,
                has_video: true,
                audio_tracks: vec![Track::OtherSide, Track::Microphone],
            })
        );
    }

    #[test]
    fn every_recording_with_video_is_written_at_1080p_whatever_its_target() {
        for target in [VideoTarget::Window(7), VideoTarget::Display(None), VideoTarget::Blank] {
            let backend = FakeBackend::default();
            backend.state().pictures.push_back(Some("first".into()));
            open(&backend, target.clone()).unwrap();

            let format = backend.state().format.clone().unwrap();
            assert_eq!((format.width, format.height), (1920, 1080), "{target:?}");
        }
    }

    #[test]
    fn the_configured_size_is_the_canvas_for_every_target_including_a_switched_window() {
        let backend = FakeBackend::default();
        backend.state().pictures.push_back(Some("first".into()));
        let config = VideoConfig {
            width: 1280,
            height: 720,
            ..video_config()
        };
        let plan = plan::meeting_plan(&request(VideoTarget::Window(7), Some(100)), &AudioConfig::default()).unwrap();
        let pipeline = Pipeline::open(
            backend.clone(),
            &plan,
            Path::new("out.mp4"),
            &config,
            &mut FakePacer::default(),
        )
        .unwrap();
        let mut control = StopAfter {
            retargets: vec![(1, 8)],
            ..StopAfter::steps(2)
        };
        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        let state = backend.state();
        assert_eq!(state.format.as_ref().map(|f| (f.width, f.height)), Some((1280, 720)));
        assert_eq!(state.canvases, [(1280, 720), (1280, 720)]);
    }

    #[test]
    fn a_track_whose_input_cannot_be_captured_is_written_as_silence_and_not_substituted() {
        let backend = FakeBackend::default();
        backend.state().failing_audio = vec![AudioSource::ProcessAudio(100)];
        backend.state().pictures.push_back(Some("first".into()));
        let pipeline = open(&backend, VideoTarget::Window(7)).unwrap();
        assert_eq!(
            backend.state().opened_audio,
            [AudioSource::Microphone(None)],
            "output device audio is not substituted"
        );
        assert_eq!(
            backend.state().format.as_ref().unwrap().audio_tracks,
            [Track::OtherSide, Track::Microphone],
            "the file keeps both tracks"
        );

        pipeline
            .run(&mut StopAfter::steps(2), &mut FakePacer::default())
            .unwrap();

        let other_side = backend.written_audio(0);
        assert_eq!(other_side.len(), 2);
        assert!(other_side.iter().flatten().all(|&sample| sample == 0), "silence");
        assert!(backend.written_audio(1).iter().flatten().all(|&sample| sample == 20));
    }

    #[test]
    fn both_tracks_are_silent_when_no_audio_input_is_available() {
        let backend = FakeBackend::default();
        backend.state().failing_audio = vec![AudioSource::ProcessAudio(100), AudioSource::Microphone(None)];
        backend.state().pictures.push_back(Some("first".into()));
        let pipeline = open(&backend, VideoTarget::Window(7)).unwrap();

        pipeline
            .run(&mut StopAfter::steps(3), &mut FakePacer::default())
            .unwrap();

        for track in [0, 1] {
            let written = backend.written_audio(track);
            assert_eq!(written.len(), 3);
            assert!(written.iter().flatten().all(|&sample| sample == 0));
        }
    }

    #[test]
    fn the_microphone_alone_is_recorded_without_video() {
        let backend = FakeBackend::default();
        let pipeline = Pipeline::open(
            backend.clone(),
            &plan::microphone_plan(&AudioConfig::default(), None),
            Path::new("mic.m4a"),
            &video_config(),
            &mut FakePacer::default(),
        )
        .unwrap();
        let format = backend.state().format.clone().unwrap();
        assert!(!format.has_video);
        assert_eq!(format.audio_tracks, [Track::Microphone]);
        assert!(backend.state().opened_video.is_empty());

        let mut control = StopAfter {
            retargets: vec![(1, 8)],
            ..StopAfter::steps(2)
        };
        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        assert!(backend.written_pictures().is_empty(), "no video is written");
        assert!(
            backend.state().opened_video.is_empty(),
            "a window cannot be given to it"
        );
        let microphone = backend.written_audio(0);
        assert_eq!(microphone.len(), 2);
        assert!(microphone.iter().flatten().all(|&sample| sample == 20));
        assert!(backend.state().finalized);
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
        let pipeline = open_request(&backend, &request(VideoTarget::Window(7), None), &mut pacer).unwrap();
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
    fn writes_each_interval_with_the_same_times_and_its_sound_once_it_has_been_captured() {
        let (backend, pipeline) = opened_window();
        pipeline
            .run(&mut StopAfter::steps(3), &mut FakePacer::default())
            .unwrap();

        let state = backend.state();
        let times: Vec<(Option<usize>, i64, i64)> = state
            .written
            .iter()
            .map(|w| match w {
                Written::Video { time, duration, .. } => (None, *time, *duration),
                Written::Audio {
                    track, time, duration, ..
                } => (Some(*track), *time, *duration),
            })
            .collect();
        assert_eq!(
            times,
            [
                (None, 0, 333_333),
                (None, 333_333, 333_333),
                (Some(0), 0, 333_333),
                (Some(1), 0, 333_333),
                (None, 666_666, 333_334),
                (Some(0), 333_333, 333_333),
                (Some(1), 333_333, 333_333),
                (Some(0), 666_666, 333_334),
                (Some(1), 666_666, 333_334),
            ],
            "each track's sound one step later; the last interval's when stopping"
        );
    }

    #[test]
    fn drops_audio_captured_before_writing_starts() {
        let (backend, pipeline) = opened_window();
        assert!(backend.state().started_writing.is_empty(), "not while opening");

        pipeline
            .run(&mut StopAfter::steps(2), &mut FakePacer::default())
            .unwrap();

        assert_eq!(
            backend.state().started_writing,
            [(AudioSource::ProcessAudio(100), 0), (AudioSource::Microphone(None), 0)],
            "every input, before anything is written"
        );
    }

    #[test]
    fn each_track_holds_only_its_own_input_one_picture_at_a_time() {
        let (backend, pipeline) = opened_window();
        backend.state().audio_level = 3;
        pipeline
            .run(&mut StopAfter::steps(1), &mut FakePacer::default())
            .unwrap();

        let other_side = backend.written_audio(0);
        let microphone = backend.written_audio(1);
        assert_eq!(other_side[0].len(), 1_600 * CHANNELS, "one picture at 30fps");
        assert!(other_side[0].iter().all(|&sample| sample == 3), "only the app's sound");
        assert!(microphone[0].iter().all(|&sample| sample == 20), "only the microphone");
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

    /// Sleeps as asked, but once `after` sleeps have passed the clock jumps by `by`, as after resuming from sleep.
    struct JumpingPacer {
        inner: FakePacer,
        after: usize,
        by: Duration,
    }

    impl Pacer for JumpingPacer {
        fn elapsed(&self) -> Duration {
            self.inner.elapsed()
        }

        fn sleep(&mut self, duration: Duration) {
            self.inner.sleep(duration);
            if self.inner.slept.len() == self.after {
                self.inner.now += self.by;
            }
        }
    }

    #[test]
    fn after_falling_far_behind_it_jumps_ahead_and_drops_the_sound_queued_meanwhile() {
        let (backend, pipeline) = opened_window();
        let mut pacer = JumpingPacer {
            inner: FakePacer::default(),
            after: 1,
            by: Duration::from_secs(10),
        };

        pipeline.run(&mut StopAfter::steps(3), &mut pacer).unwrap();

        let state = backend.state();
        assert_eq!(
            state.started_writing.len(),
            4,
            "both inputs at the start and again after the jump: {:?}",
            state.started_writing
        );
        let audio_times: Vec<i64> = state
            .written
            .iter()
            .filter_map(|w| match w {
                Written::Audio { time, .. } => Some(*time),
                Written::Video { .. } => None,
            })
            .collect();
        assert_eq!(
            audio_times,
            [0, 0, 100_000_000, 100_000_000],
            "each track once per interval; the interval before the jump is not written after it"
        );
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
            Some((1920, 1080)),
            "the size does not change"
        );
    }

    #[test]
    fn opens_only_the_last_of_several_queued_window_switches() {
        let (backend, pipeline) = opened_window();
        let mut control = StopAfter {
            retargets: vec![(1, 8), (1, 9), (1, 10)],
            ..StopAfter::steps(2)
        };

        pipeline.run(&mut control, &mut FakePacer::default()).unwrap();

        assert_eq!(
            backend.state().opened_video,
            [VideoSource::Window(7), VideoSource::Window(10)]
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
