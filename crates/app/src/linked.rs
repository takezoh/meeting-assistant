//! Combines detection and recording through OS features with detection and recording through the browser extension
//! into a single signal source and a single recorder.
//!
//! A browser tab is recorded by the extension, and the user's microphone by the app beside it, because the browser does
//! not let the extension record the microphone. When the tab recording ends, the microphone is added to it as its second
//! audio track.
//!
//! Every recording, whichever side made it, then gets what is known of it written into its file, in the same way.

use std::path::{Path, PathBuf};
use std::time::Instant;

use meeting_assistant_core::{
    AudioSession, BrowserLink, BrowserMeeting, BrowserTab, MissingSound, ProcessEntry, Recorder, RecordingMetadata,
    RecordingRequest, SignalSource, Sound, Started, VideoTarget, WindowEntry,
};

use crate::recording::annotate::{self, Annotate};
use crate::recording::merge::{self, MicrophoneMerge, MicrophoneRecorder};

/// A tab recording that has started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabCapture {
    /// The file being written: the requested path with the extension of the container the browser records in.
    pub path: PathBuf,
    /// When the app accepted the upload, which the extension opens as it starts recording: the recording's time zero.
    pub started: Instant,
}

/// Port to the browser extension.
pub trait BrowserPort {
    /// The connected extension and the meetings it reports.
    fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>);
    /// Makes the extension start recording a tab and write it to `path`. Returns once recording has started.
    fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<TabCapture>;
    /// Stops recording and finalizes the file. Does nothing if not recording.
    fn stop_capture(&self) -> anyhow::Result<()>;
    /// If recording failed without a stop request, returns the cause exactly once.
    fn take_failure(&self) -> Option<String>;
}

/// Adds the meetings reported by the browser extension to the signals taken from the OS.
pub struct LinkedSignalSource<S, B> {
    platform: S,
    browser: Option<B>,
}

impl<S, B> LinkedSignalSource<S, B> {
    pub fn new(platform: S, browser: Option<B>) -> Self {
        Self { platform, browser }
    }
}

impl<S: SignalSource, B: BrowserPort> SignalSource for LinkedSignalSource<S, B> {
    fn capture_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.platform.capture_sessions()
    }

    fn render_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.platform.render_sessions()
    }

    fn processes(&mut self) -> anyhow::Result<Vec<ProcessEntry>> {
        self.platform.processes()
    }

    fn windows(&mut self) -> anyhow::Result<Vec<WindowEntry>> {
        self.platform.windows()
    }

    fn browser(&mut self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        self.browser
            .as_ref()
            .map_or_else(|| (Vec::new(), Vec::new()), BrowserPort::snapshot)
    }
}

/// The microphone being recorded beside a tab recording.
struct SideMicrophone {
    path: PathBuf,
    started: Instant,
}

enum Active {
    Platform {
        /// The file being written.
        path: PathBuf,
    },
    Browser {
        capture: TabCapture,
        microphone: Option<SideMicrophone>,
    },
}

/// Records through either OS features or the browser extension, depending on the recording target.
pub struct LinkedRecorder<R, B, M, G, A> {
    platform: R,
    browser: Option<B>,
    microphone: M,
    merge: G,
    annotate: A,
    active: Option<Active>,
    /// What is written into the file being recorded once it is finished, as last told.
    metadata: RecordingMetadata,
}

impl<R, B, M, G, A> LinkedRecorder<R, B, M, G, A> {
    /// `microphone` records the microphone beside a tab recording, and `merge` adds it to the tab recording. `annotate`
    /// writes what is known of each recording into its file once it is finished.
    pub fn new(platform: R, browser: Option<B>, microphone: M, merge: G, annotate: A) -> Self {
        Self {
            platform,
            browser,
            microphone,
            merge,
            annotate,
            active: None,
            metadata: RecordingMetadata::default(),
        }
    }
}

impl<R, B, M, G, A: Annotate> LinkedRecorder<R, B, M, G, A> {
    /// Writes what is known of a finished recording into its file. The recording is kept as it is if that fails.
    fn write_metadata(&self, recording: &Path) {
        if !annotate::can_describe(recording) {
            log::info!(
                "The recording is not MP4, so what is known of it was not written into it: {}",
                recording.display()
            );
            return;
        }
        match self.annotate.annotate(recording, &self.metadata) {
            Ok(()) => log::info!("Wrote what is known of the recording into its file."),
            Err(error) => log::warn!(
                "Could not write what is known of the recording into {}: {error:#}",
                recording.display()
            ),
        }
    }
}

/// Starts the microphone beside a tab recording about to start at `recording`. The tab is recorded without it if it
/// cannot be started. Also returns the microphone as missing when it could not be started or opened.
fn start_side_microphone(
    microphone: &mut impl MicrophoneRecorder,
    recording: &Path,
    device: Option<String>,
) -> (Option<SideMicrophone>, Vec<MissingSound>) {
    let path = merge::microphone_path(recording);
    match microphone.start_microphone(&path, device) {
        Ok(ready) => (
            Some(SideMicrophone {
                path,
                started: ready.zero,
            }),
            ready.missing,
        ),
        Err(error) => {
            log::warn!("Your own voice will not be in the tab recording: cannot record the microphone: {error:#}");
            let missing = MissingSound {
                sound: Sound::Microphone,
                reason: format!("{error:#}"),
            };
            (None, vec![missing])
        }
    }
}

impl<R, B, M: MicrophoneRecorder, G: MicrophoneMerge, A> LinkedRecorder<R, B, M, G, A> {
    /// Ends the microphone recorded beside a tab recording that has ended, and adds it to that recording.
    fn finish_tab(&mut self, capture: &TabCapture, microphone: Option<SideMicrophone>) {
        let Some(microphone) = microphone else { return };
        if let Err(error) = self.microphone.stop_microphone() {
            log::warn!("The microphone recording ended early; what it recorded is kept: {error:#}");
        }
        if !merge::can_add_to(&capture.path) {
            log::warn!(
                "The browser did not record MP4, so your own voice was not added to the recording. It is kept beside it: {}",
                microphone.path.display()
            );
            return;
        }
        let placement = merge::placement(capture.started, microphone.started);
        match self.merge.add_microphone(&capture.path, &microphone.path, placement) {
            Ok(()) => log::info!("Added your own voice to the recording as its second audio track."),
            Err(error) => log::warn!(
                "Could not add your own voice to {}; it is kept beside it: {}: {error:#}",
                capture.path.display(),
                microphone.path.display()
            ),
        }
    }
}

impl<R: Recorder, B: BrowserPort, M: MicrophoneRecorder, G: MicrophoneMerge, A: Annotate> Recorder
    for LinkedRecorder<R, B, M, G, A>
{
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<Started> {
        anyhow::ensure!(self.active.is_none(), "already recording");
        self.metadata = request.metadata.clone();
        match &request.video {
            VideoTarget::BrowserTab(tab) => {
                let Some(browser) = &self.browser else {
                    anyhow::bail!("not connected to the browser extension");
                };
                // The microphone starts first, so that it covers the whole tab recording.
                let (microphone, missing) = start_side_microphone(
                    &mut self.microphone,
                    &request.output_path,
                    request.capture_device_id.clone(),
                );
                match browser.start_capture(*tab, &request.output_path) {
                    Ok(capture) => {
                        let path = capture.path.clone();
                        self.active = Some(Active::Browser { capture, microphone });
                        Ok(Started { path, missing })
                    }
                    Err(error) => {
                        if let Some(microphone) = microphone {
                            let _ = self.microphone.stop_microphone();
                            self.merge.discard(&microphone.path);
                        }
                        Err(error)
                    }
                }
            }
            _ => {
                let started = self.platform.start(request)?;
                self.active = Some(Active::Platform {
                    path: started.path.clone(),
                });
                Ok(started)
            }
        }
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        let (stopped, written) = match self.active.take() {
            Some(Active::Platform { path }) => (self.platform.stop(), path),
            Some(Active::Browser { capture, microphone }) => {
                let stopped = self.browser.as_ref().map_or(Ok(()), B::stop_capture);
                self.finish_tab(&capture, microphone);
                (stopped, capture.path)
            }
            None => return Ok(()),
        };
        self.write_metadata(&written);
        stopped
    }

    fn retarget(&mut self, window: u64) -> anyhow::Result<()> {
        match self.active {
            Some(Active::Platform { .. }) => self.platform.retarget(window),
            // A tab recording is not something whose window can be switched.
            Some(Active::Browser { .. }) | None => Ok(()),
        }
    }

    fn describe(&mut self, metadata: &RecordingMetadata) {
        if self.active.is_some() {
            self.metadata = metadata.clone();
        }
    }

    fn take_failure(&mut self) -> Option<String> {
        let failure = match self.active.as_ref()? {
            Active::Platform { .. } => self.platform.take_failure(),
            Active::Browser { .. } => self.browser.as_ref().and_then(B::take_failure),
        }?;
        // A failed recording keeps what was written, and gets what is known of it like any other.
        let written = match self.active.take()? {
            Active::Platform { path } => path,
            Active::Browser { capture, microphone } => {
                // The tab recording keeps what arrived; it gets the microphone for that time too.
                self.finish_tab(&capture, microphone);
                capture.path
            }
        };
        self.write_metadata(&written);
        Some(failure)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use meeting_assistant_core::fakes::{FakeRecorder, ScriptedSignals, zoom_in_meeting};
    use meeting_assistant_core::full_snapshot;

    use super::*;
    use crate::recording::merge::Placement;
    use crate::recording::session::Ready;

    const TAB: BrowserTab = BrowserTab { link: 1, tab: 42 };

    /// What the fakes were asked to do, in order.
    type Journal = Arc<Mutex<Vec<String>>>;

    fn note(journal: &Journal, entry: impl Into<String>) {
        journal.lock().unwrap().push(entry.into());
    }

    struct BrowserState {
        journal: Journal,
        /// The time zero each started recording reports.
        zero: Instant,
        /// The extension of the file the browser writes.
        container: &'static str,
        start_fails: bool,
        failure: Option<String>,
        meetings: Vec<BrowserMeeting>,
    }

    /// A fake browser extension that only records the instructions it receives.
    #[derive(Clone)]
    struct FakeBrowser(Arc<Mutex<BrowserState>>);

    impl FakeBrowser {
        fn new(journal: &Journal, zero: Instant) -> Self {
            Self(Arc::new(Mutex::new(BrowserState {
                journal: journal.clone(),
                zero,
                container: "mp4",
                start_fails: false,
                failure: None,
                meetings: Vec::new(),
            })))
        }
    }

    impl BrowserPort for FakeBrowser {
        fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
            (
                vec![BrowserLink {
                    id: 1,
                    host_process_id: 301,
                }],
                self.0.lock().unwrap().meetings.clone(),
            )
        }
        fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<TabCapture> {
            let state = self.0.lock().unwrap();
            anyhow::ensure!(!state.start_fails, "not allowed");
            note(&state.journal, format!("tab {} started: {}", tab.tab, path.display()));
            Ok(TabCapture {
                path: path.with_extension(state.container),
                started: state.zero,
            })
        }
        fn stop_capture(&self) -> anyhow::Result<()> {
            note(&self.0.lock().unwrap().journal, "tab stopped");
            Ok(())
        }
        fn take_failure(&self) -> Option<String> {
            self.0.lock().unwrap().failure.take()
        }
    }

    struct FakeMicrophone {
        journal: Journal,
        zero: Instant,
        start_fails: bool,
        stop_fails: bool,
        /// While `Some`, the microphone recording starts silent, with this reason.
        cannot_open: Option<String>,
    }

    impl MicrophoneRecorder for Arc<Mutex<FakeMicrophone>> {
        fn start_microphone(&mut self, path: &Path, device: Option<String>) -> anyhow::Result<Ready> {
            let state = self.lock().unwrap();
            anyhow::ensure!(!state.start_fails, "no microphone permission");
            note(
                &state.journal,
                format!("microphone started: {} ({device:?})", path.display()),
            );
            Ok(Ready {
                zero: state.zero,
                missing: state
                    .cannot_open
                    .iter()
                    .map(|reason| MissingSound {
                        sound: Sound::Microphone,
                        reason: reason.clone(),
                    })
                    .collect(),
            })
        }
        fn stop_microphone(&mut self) -> anyhow::Result<()> {
            let state = self.lock().unwrap();
            note(&state.journal, "microphone stopped");
            anyhow::ensure!(!state.stop_fails, "device lost");
            Ok(())
        }
    }

    struct FakeMerge {
        journal: Journal,
        fails: bool,
    }

    impl MicrophoneMerge for Arc<Mutex<FakeMerge>> {
        fn add_microphone(&self, recording: &Path, microphone: &Path, placement: Placement) -> anyhow::Result<()> {
            let state = self.lock().unwrap();
            note(
                &state.journal,
                format!(
                    "added {} to {} skipping {:?}",
                    microphone.display(),
                    recording.display(),
                    placement.skip
                ),
            );
            anyhow::ensure!(!state.fails, "cannot read");
            Ok(())
        }
        fn discard(&self, microphone: &Path) {
            note(
                &self.lock().unwrap().journal,
                format!("discarded {}", microphone.display()),
            );
        }
    }

    /// Records what each finished recording was described as, with how many entries the journal had by then.
    #[derive(Clone)]
    struct FakeAnnotate {
        journal: Journal,
        written: Arc<Mutex<Vec<(String, RecordingMetadata, usize)>>>,
    }

    impl Annotate for FakeAnnotate {
        fn annotate(&self, recording: &Path, metadata: &RecordingMetadata) -> anyhow::Result<()> {
            let after = self.journal.lock().unwrap().len();
            self.written
                .lock()
                .unwrap()
                .push((recording.display().to_string(), metadata.clone(), after));
            Ok(())
        }
    }

    type Linked =
        LinkedRecorder<FakeRecorder, FakeBrowser, Arc<Mutex<FakeMicrophone>>, Arc<Mutex<FakeMerge>>, FakeAnnotate>;

    struct Fixture {
        linked: Linked,
        platform: FakeRecorder,
        browser: FakeBrowser,
        microphone: Arc<Mutex<FakeMicrophone>>,
        merge: Arc<Mutex<FakeMerge>>,
        annotate: FakeAnnotate,
        journal: Journal,
    }

    impl Fixture {
        fn journal(&self) -> Vec<String> {
            self.journal.lock().unwrap().clone()
        }

        fn annotated(&self) -> Vec<(String, RecordingMetadata, usize)> {
            self.annotate.written.lock().unwrap().clone()
        }
    }

    /// The microphone's time zero is 250 ms before the tab's.
    fn fixture() -> Fixture {
        let journal = Journal::default();
        let microphone_zero = Instant::now();
        let platform = FakeRecorder::default();
        let browser = FakeBrowser::new(&journal, microphone_zero + Duration::from_millis(250));
        let microphone = Arc::new(Mutex::new(FakeMicrophone {
            journal: journal.clone(),
            zero: microphone_zero,
            start_fails: false,
            stop_fails: false,
            cannot_open: None,
        }));
        let merge = Arc::new(Mutex::new(FakeMerge {
            journal: journal.clone(),
            fails: false,
        }));
        let annotate = FakeAnnotate {
            journal: journal.clone(),
            written: Arc::default(),
        };
        Fixture {
            linked: LinkedRecorder::new(
                platform.clone(),
                Some(browser.clone()),
                microphone.clone(),
                merge.clone(),
                annotate.clone(),
            ),
            platform,
            browser,
            microphone,
            merge,
            annotate,
            journal,
        }
    }

    fn described(app: &str, conversation: &str) -> RecordingMetadata {
        RecordingMetadata {
            app: app.into(),
            started_at: None,
            meeting: meeting_assistant_core::MeetingMetadata::new([("conversation", conversation)]),
        }
    }

    fn request(video: VideoTarget) -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video,
            audio_process_id: None,
            render_device_id: None,
            capture_device_id: Some("mic".into()),
            microphone: true,
            metadata: described("Zoom", "at start"),
        }
    }

    const MERGED: &str = "added out.microphone.m4a to out.mp4 skipping 250ms";

    #[test]
    fn a_tab_is_recorded_by_the_extension_with_the_microphone_recorded_beside_it_and_added_when_it_ends() {
        let mut f = fixture();
        let started = f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        assert_eq!(
            started,
            Started {
                path: PathBuf::from("out.mp4"),
                missing: Vec::new(),
            },
            "the file the extension writes is reported, with the microphone"
        );
        assert!(f.platform.state().started.is_empty());
        assert_eq!(
            f.journal(),
            [
                "microphone started: out.microphone.m4a (Some(\"mic\"))",
                "tab 42 started: out.mp4"
            ],
            "the microphone starts first, so that it covers the whole tab recording"
        );

        f.linked.stop().unwrap();
        assert_eq!(
            f.journal()[2..],
            ["tab stopped", "microphone stopped", MERGED],
            "the part of the microphone before the tab's start is dropped"
        );
        assert_eq!(f.platform.state().stop_count, 0);
    }

    #[test]
    fn other_recording_is_delegated_to_os_features() {
        for video in [VideoTarget::Window(7), VideoTarget::Display(None), VideoTarget::Blank] {
            let mut f = fixture();
            f.linked.start(&request(video.clone())).unwrap();

            assert_eq!(f.platform.state().started.len(), 1, "{video:?}");
            f.linked.stop().unwrap();
            assert_eq!(f.platform.state().stop_count, 1);
            assert!(f.journal().is_empty(), "no tab and no separate microphone: {video:?}");
        }
    }

    #[test]
    fn a_tab_that_cannot_start_leaves_no_microphone_recording() {
        let mut f = fixture();
        f.browser.0.lock().unwrap().start_fails = true;

        assert!(f.linked.start(&request(VideoTarget::BrowserTab(TAB))).is_err());

        assert_eq!(f.journal()[1..], ["microphone stopped", "discarded out.microphone.m4a"]);
        f.linked.start(&request(VideoTarget::Display(None))).unwrap();
        assert_eq!(f.platform.state().started.len(), 1, "it is not left recording");
    }

    #[test]
    fn a_tab_is_still_recorded_when_the_microphone_cannot_be_started() {
        let mut f = fixture();
        f.microphone.lock().unwrap().start_fails = true;

        let started = f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();

        assert_eq!(f.journal(), ["tab 42 started: out.mp4", "tab stopped"]);
        assert_eq!(
            started.missing,
            [MissingSound {
                sound: Sound::Microphone,
                reason: "no microphone permission".into(),
            }],
            "the start says the microphone is missing, and why"
        );
    }

    #[test]
    fn a_microphone_recording_that_starts_silent_is_reported_with_the_tab_recording() {
        let mut f = fixture();
        f.microphone.lock().unwrap().cannot_open = Some("no input device".into());

        let started = f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        let missing: Vec<&str> = started.missing.iter().map(|m| m.reason.as_str()).collect();
        assert_eq!(missing, ["no input device"]);
        f.linked.stop().unwrap();
    }

    #[test]
    fn what_the_os_recording_lacks_is_passed_on() {
        let mut f = fixture();
        let lacking = MissingSound {
            sound: Sound::OtherSide,
            reason: "screen recording is not allowed".into(),
        };
        f.platform.state().missing = vec![lacking.clone()];

        let started = f.linked.start(&request(VideoTarget::Window(7))).unwrap();

        assert_eq!(started.missing, [lacking]);
        f.linked.stop().unwrap();
    }

    #[test]
    fn a_microphone_that_ended_early_is_still_added() {
        let mut f = fixture();
        f.microphone.lock().unwrap().stop_fails = true;

        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();

        assert_eq!(f.journal().last().unwrap(), MERGED);
    }

    #[test]
    fn a_failed_merge_does_not_fail_the_recording() {
        let mut f = fixture();
        f.merge.lock().unwrap().fails = true;

        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();

        assert_eq!(f.journal().last().unwrap(), MERGED);
    }

    #[test]
    fn a_tab_recorded_as_webm_keeps_the_microphone_beside_it() {
        let mut f = fixture();
        f.browser.0.lock().unwrap().container = "webm";

        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();

        assert_eq!(f.journal()[2..], ["tab stopped", "microphone stopped"]);
    }

    #[test]
    fn a_failed_tab_recording_still_gets_the_microphone_and_is_reported_once() {
        let mut f = fixture();
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        f.platform.fail("unrelated");
        assert_eq!(f.linked.take_failure(), None, "only the side that is recording reports");

        f.browser.0.lock().unwrap().failure = Some("tab closed".into());
        assert_eq!(f.linked.take_failure(), Some("tab closed".into()));
        assert_eq!(f.journal()[2..], ["microphone stopped", MERGED]);
        assert_eq!(f.linked.take_failure(), None);
        f.linked.stop().unwrap();
        assert_eq!(f.journal().len(), 4, "nothing is stopped twice");
    }

    #[test]
    fn every_finished_recording_gets_what_was_last_told_of_it_written_into_its_file() {
        // Through OS features.
        let mut f = fixture();
        f.linked.describe(&described("Zoom", "before start"));
        f.linked.start(&request(VideoTarget::Window(7))).unwrap();
        f.linked.describe(&described("Zoom", "learned later"));
        f.linked.stop().unwrap();
        let annotated = f.annotated();
        assert_eq!(annotated.len(), 1);
        assert_eq!(annotated[0].0, "out.mp4");
        assert_eq!(annotated[0].1, described("Zoom", "learned later"));

        // A tab, once the microphone has been added to it.
        let mut f = fixture();
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();
        let annotated = f.annotated();
        assert_eq!(annotated.len(), 1);
        assert_eq!(
            (annotated[0].0.as_str(), &annotated[0].1),
            ("out.mp4", &described("Zoom", "at start"))
        );
        assert_eq!(f.journal()[annotated[0].2 - 1], MERGED, "written after the merge");
    }

    #[test]
    fn a_failed_recording_also_gets_it_and_a_recording_other_than_mp4_does_not() {
        let mut f = fixture();
        f.linked.start(&request(VideoTarget::Window(7))).unwrap();
        f.platform.fail("device lost");
        f.linked.take_failure().unwrap();
        assert_eq!(f.annotated().len(), 1);

        let mut f = fixture();
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.browser.0.lock().unwrap().failure = Some("tab closed".into());
        f.linked.take_failure().unwrap();
        let annotated = f.annotated();
        assert_eq!(f.journal()[annotated[0].2 - 1], MERGED);

        let mut f = fixture();
        f.browser.0.lock().unwrap().container = "webm";
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.stop().unwrap();
        assert!(f.annotated().is_empty());
    }

    #[test]
    fn does_not_start_another_recording_while_recording() {
        let mut f = fixture();
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        assert!(f.linked.start(&request(VideoTarget::Display(None))).is_err());
        assert!(f.platform.state().started.is_empty());
    }

    #[test]
    fn tab_cannot_be_recorded_without_extension_connection() {
        let f = fixture();
        let mut linked = LinkedRecorder::<_, FakeBrowser, _, _, _>::new(
            FakeRecorder::default(),
            None,
            f.microphone,
            f.merge,
            f.annotate,
        );

        assert!(linked.start(&request(VideoTarget::BrowserTab(TAB))).is_err());
        assert!(f.journal.lock().unwrap().is_empty(), "the microphone is not started");
    }

    #[test]
    fn can_start_next_recording_after_failure_is_reported() {
        let mut f = fixture();
        f.linked.start(&request(VideoTarget::Window(7))).unwrap();
        f.platform.fail("device lost");

        assert_eq!(f.linked.take_failure(), Some("device lost".into()));
        f.linked.start(&request(VideoTarget::Window(7))).unwrap();
    }

    #[test]
    fn window_switching_happens_only_while_recording_through_os_features() {
        let mut f = fixture();
        f.linked.retarget(1).unwrap();
        f.linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        f.linked.retarget(2).unwrap();
        f.linked.stop().unwrap();
        f.linked.start(&request(VideoTarget::Window(7))).unwrap();
        f.linked.retarget(3).unwrap();

        assert_eq!(f.platform.state().retargets, [3]);
    }

    #[test]
    fn signals_include_meetings_reported_by_the_extension() {
        let f = fixture();
        let signals = ScriptedSignals::default();
        signals.set(zoom_in_meeting());
        f.browser.0.lock().unwrap().meetings = vec![BrowserMeeting {
            tab: TAB,
            service: "GoogleMeet".into(),
            metadata: meeting_assistant_core::MeetingMetadata::default(),
        }];

        let snapshot = full_snapshot(&mut LinkedSignalSource::new(signals, Some(f.browser))).unwrap();

        assert_eq!(
            snapshot.capture_sessions.len(),
            1,
            "OS signals are passed through unchanged"
        );
        assert_eq!(snapshot.browser_links.len(), 1);
        assert_eq!(snapshot.browser_meetings[0].service, "GoogleMeet");
    }

    #[test]
    fn returns_only_os_signals_when_not_linked_with_the_extension() {
        let signals = ScriptedSignals::default();
        signals.set(zoom_in_meeting());

        let snapshot = full_snapshot(&mut LinkedSignalSource::<_, FakeBrowser>::new(signals, None)).unwrap();

        assert!(snapshot.browser_links.is_empty() && snapshot.browser_meetings.is_empty());
    }

    #[test]
    fn reports_failure_when_os_signals_cannot_be_obtained() {
        let f = fixture();
        let signals = ScriptedSignals::default();
        signals.set_failure(Some("audio service gone"));

        assert!(full_snapshot(&mut LinkedSignalSource::new(signals, Some(f.browser))).is_err());
    }
}
