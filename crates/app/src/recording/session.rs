//! Runs a recording on a dedicated thread and exposes it as a `Recorder`.
//!
//! The actual capture and writing ([`CaptureBackend`]) is created inside the recording thread
//! (because some OS capture objects are bound to the thread that created them).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use meeting_assistant_core::{AudioConfig, Recorder, RecordingRequest, VideoConfig};

use super::pipeline::{CaptureBackend, Control, Pacer, Pipeline};

/// Upper limit on waiting for recording preparation (starting capture, creating the output, the first picture).
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// Waits using the real clock.
struct RealPacer(Instant);

impl Pacer for RealPacer {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Passes instructions from other threads to the recording thread.
struct ThreadControl {
    stop: Arc<AtomicBool>,
    retarget: mpsc::Receiver<u64>,
}

impl Control for ThreadControl {
    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn take_retarget(&mut self) -> Option<u64> {
        self.retarget.try_recv().ok()
    }
}

struct Session {
    stop: Arc<AtomicBool>,
    retarget: mpsc::Sender<u64>,
    thread: JoinHandle<anyhow::Result<()>>,
}

/// A recorder that runs each recording on its own thread.
pub struct ThreadedRecorder<B> {
    video: VideoConfig,
    audio: AudioConfig,
    /// Called inside the recording thread to create the actual capture and writing.
    create_backend: fn() -> anyhow::Result<B>,
    session: Option<Session>,
}

impl<B> ThreadedRecorder<B> {
    pub fn new(video: VideoConfig, audio: AudioConfig, create_backend: fn() -> anyhow::Result<B>) -> Self {
        Self {
            video,
            audio,
            create_backend,
            session: None,
        }
    }
}

impl<B: CaptureBackend + 'static> Recorder for ThreadedRecorder<B> {
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(self.session.is_none(), "already recording");

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (retarget_sender, retarget_receiver) = mpsc::channel();
        let thread = std::thread::Builder::new().name("recorder".into()).spawn({
            let (request, video, audio) = (request.clone(), self.video.clone(), self.audio.clone());
            let create_backend = self.create_backend;
            let control = ThreadControl {
                stop: stop.clone(),
                retarget: retarget_receiver,
            };
            move || record(create_backend, &request, &video, &audio, control, ready_sender)
        })?;

        match ready_receiver.recv_timeout(START_TIMEOUT) {
            Ok(Ok(())) => {
                self.session = Some(Session {
                    stop,
                    retarget: retarget_sender,
                    thread,
                });
                Ok(request.output_path.clone())
            }
            Ok(Err(error)) => Err(error),
            Err(_) => {
                // Once preparation finishes, the thread sees this signal and exits on its own.
                stop.store(true, Ordering::Relaxed);
                anyhow::bail!("recording did not finish starting in time")
            }
        }
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        let Some(session) = self.session.take() else {
            return Ok(());
        };
        session.stop.store(true, Ordering::Relaxed);
        session
            .thread
            .join()
            .map_err(|_| anyhow::anyhow!("the recording thread terminated abnormally"))?
    }

    fn retarget(&mut self, window: u64) -> anyhow::Result<()> {
        let session = self.session.as_ref().ok_or_else(|| anyhow::anyhow!("not recording"))?;
        // The recording thread switches at its next cycle. If that fails, recording continues with the previous video.
        session
            .retarget
            .send(window)
            .map_err(|_| anyhow::anyhow!("the recording thread has exited"))
    }

    fn take_failure(&mut self) -> Option<String> {
        if !self.session.as_ref()?.thread.is_finished() {
            return None;
        }
        // The thread has finished although no stop was requested.
        let session = self.session.take()?;
        Some(match session.thread.join() {
            Ok(Ok(())) => "recording ended unexpectedly".to_owned(),
            Ok(Err(error)) => format!("{error:#}"),
            Err(_) => "the recording thread terminated abnormally".to_owned(),
        })
    }
}

impl<B> Drop for ThreadedRecorder<B> {
    fn drop(&mut self) {
        // If dropped while recording, still finalize the file before exiting.
        if let Some(session) = self.session.take() {
            session.stop.store(true, Ordering::Relaxed);
            match session.thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::error!("failed to stop recording: {error:#}"),
                Err(_) => log::error!("the recording thread terminated abnormally"),
            }
        }
    }
}

/// Body of the recording thread. Reports whether preparation succeeded via `ready`, then writes until told to stop.
/// The return value is the result after preparation (a preparation failure is reported via `ready`, so it is `Ok`).
fn record<B: CaptureBackend>(
    create_backend: fn() -> anyhow::Result<B>,
    request: &RecordingRequest,
    video: &VideoConfig,
    audio: &AudioConfig,
    mut control: ThreadControl,
    ready: mpsc::Sender<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let mut pacer = RealPacer(Instant::now());
    let opened = create_backend().and_then(|backend| Pipeline::open(backend, request, video, audio, &mut pacer));
    let pipeline = match opened {
        Ok(pipeline) => {
            let _ = ready.send(Ok(()));
            pipeline
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    pipeline.run(&mut control, &mut pacer)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use meeting_assistant_core::VideoTarget;

    use super::super::pipeline::fakes::FakeBackend;
    use super::super::plan::VideoSource;
    use super::*;

    const PATIENCE: Duration = Duration::from_secs(10);

    /// A hand-off slot for tests to inspect the fake created by the recording thread.
    /// It can only be passed as a function pointer, so each test uses its own slot.
    macro_rules! shared_backend {
        () => {{
            static SHARED: OnceLock<Mutex<Option<FakeBackend>>> = OnceLock::new();
            fn create() -> anyhow::Result<FakeBackend> {
                SHARED
                    .get()
                    .and_then(|slot| slot.lock().unwrap().clone())
                    .ok_or_else(|| anyhow::anyhow!("no backend"))
            }
            let backend = FakeBackend::default();
            // Put it in a state where the display's first picture has already arrived.
            backend.state().pictures.push_back(Some("first".into()));
            *SHARED.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(backend.clone());
            (backend, create as fn() -> anyhow::Result<FakeBackend>)
        }};
    }

    fn request() -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video: VideoTarget::Display(None),
            audio_process_id: None,
            render_device_id: None,
            capture_device_id: None,
        }
    }

    fn recorder(create: fn() -> anyhow::Result<FakeBackend>) -> ThreadedRecorder<FakeBackend> {
        let video = VideoConfig {
            framerate: 60,
            quality: 70,
            fallback_bitrate_kbps: 4000,
        };
        ThreadedRecorder::new(video, AudioConfig::default(), create)
    }

    fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + PATIENCE;
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn start_returns_after_preparation_and_stop_finalizes_the_file() {
        let (backend, create) = shared_backend!();
        let mut recorder = recorder(create);

        recorder.start(&request()).unwrap();
        assert!(
            backend.state().format.is_some(),
            "by the time start returns, the output is open"
        );
        wait_until("writing progresses", || backend.state().written.len() >= 4);
        assert!(!backend.state().finalized);

        recorder.stop().unwrap();
        assert!(backend.state().finalized);
        assert_eq!(recorder.take_failure(), None);
    }

    #[test]
    fn start_fails_with_the_reason_when_preparation_fails() {
        let (backend, create) = shared_backend!();
        backend.state().failing_video = vec![VideoSource::Display(None)];
        let mut recorder = recorder(create);

        let error = recorder.start(&request()).unwrap_err();

        assert!(format!("{error:#}").contains("cannot capture"), "{error:#}");
        assert_eq!(
            recorder.take_failure(),
            None,
            "a start failure is not reported as a failure during recording"
        );
        recorder.stop().unwrap();
    }

    #[test]
    fn cannot_start_a_second_recording_while_recording() {
        let (_backend, create) = shared_backend!();
        let mut recorder = recorder(create);
        recorder.start(&request()).unwrap();

        assert!(recorder.start(&request()).is_err());
        recorder.stop().unwrap();
    }

    #[test]
    fn reports_a_failure_during_recording_once_and_allows_the_next_recording() {
        let (backend, create) = shared_backend!();
        let mut recorder = recorder(create);
        recorder.start(&request()).unwrap();
        assert_eq!(recorder.take_failure(), None);

        backend.state().video_broken = true;
        let mut failure = None;
        wait_until("failure is reported", || {
            failure = failure.take().or_else(|| recorder.take_failure());
            failure.is_some()
        });

        assert!(failure.unwrap().contains("display lost"));
        assert_eq!(recorder.take_failure(), None);
        backend.state().video_broken = false;
        backend.state().pictures.push_back(Some("again".into()));
        recorder.start(&request()).unwrap();
        recorder.stop().unwrap();
    }

    #[test]
    fn forwards_video_target_switch_to_the_recording_thread() {
        let (backend, create) = shared_backend!();
        let mut recorder = recorder(create);
        assert!(recorder.retarget(8).is_err(), "cannot switch when not recording");

        recorder.start(&request()).unwrap();
        recorder.retarget(8).unwrap();
        wait_until("target switches", || {
            backend.state().opened_video.contains(&VideoSource::Window(8))
        });
        recorder.stop().unwrap();
    }

    #[test]
    fn finalizes_the_file_even_if_dropped_while_recording() {
        let (backend, create) = shared_backend!();
        let mut recorder = recorder(create);
        recorder.start(&request()).unwrap();

        drop(recorder);

        assert!(backend.state().finalized);
    }

    #[test]
    fn stop_does_nothing_when_not_recording() {
        let (backend, create) = shared_backend!();
        let mut recorder = recorder(create);

        recorder.stop().unwrap();

        assert!(backend.state().format.is_none());
    }
}
