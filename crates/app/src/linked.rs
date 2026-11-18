//! Combines detection and recording through OS features with detection and recording through the browser extension
//! into a single signal source and a single recorder.

use std::path::{Path, PathBuf};

use meeting_assistant_core::{
    BrowserLink, BrowserMeeting, BrowserTab, Recorder, RecordingRequest, SignalSnapshot, SignalSource, VideoTarget,
};

/// Port to the browser extension.
pub trait BrowserPort {
    /// The connected extension and the meetings it reports.
    fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>);
    /// Makes the extension start recording a tab and write it to `path`. Returns once recording has started, with the
    /// file written: `path` with the extension of the container the browser records in.
    fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<PathBuf>;
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
    fn capture(&mut self) -> anyhow::Result<SignalSnapshot> {
        let mut snapshot = self.platform.capture()?;
        if let Some(browser) = &self.browser {
            (snapshot.browser_links, snapshot.browser_meetings) = browser.snapshot();
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Copy)]
enum Active {
    Platform,
    Browser,
}

/// Records through either OS features or the browser extension, depending on the recording target.
pub struct LinkedRecorder<R, B> {
    platform: R,
    browser: Option<B>,
    active: Option<Active>,
}

impl<R, B> LinkedRecorder<R, B> {
    pub fn new(platform: R, browser: Option<B>) -> Self {
        Self {
            platform,
            browser,
            active: None,
        }
    }
}

impl<R: Recorder, B: BrowserPort> Recorder for LinkedRecorder<R, B> {
    fn start(&mut self, request: &RecordingRequest) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(self.active.is_none(), "already recording");
        match &request.video {
            VideoTarget::BrowserTab(tab) => {
                let browser = self
                    .browser
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("not connected to the browser extension"))?;
                let written = browser.start_capture(*tab, &request.output_path)?;
                self.active = Some(Active::Browser);
                Ok(written)
            }
            _ => {
                let written = self.platform.start(request)?;
                self.active = Some(Active::Platform);
                Ok(written)
            }
        }
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        match self.active.take() {
            Some(Active::Platform) => self.platform.stop(),
            Some(Active::Browser) => self.browser.as_ref().map_or(Ok(()), B::stop_capture),
            None => Ok(()),
        }
    }

    fn retarget(&mut self, window: u64) -> anyhow::Result<()> {
        match self.active {
            Some(Active::Platform) => self.platform.retarget(window),
            // A tab recording is not something whose window can be switched.
            Some(Active::Browser) | None => Ok(()),
        }
    }

    fn take_failure(&mut self) -> Option<String> {
        let failure = match self.active? {
            Active::Platform => self.platform.take_failure(),
            Active::Browser => self.browser.as_ref().and_then(B::take_failure),
        }?;
        self.active = None;
        Some(failure)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use meeting_assistant_core::fakes::{FakeRecorder, ScriptedSignals, zoom_in_meeting};

    use super::*;

    const TAB: BrowserTab = BrowserTab { link: 1, tab: 42 };

    #[derive(Default)]
    struct BrowserState {
        started: Vec<(BrowserTab, PathBuf)>,
        stopped: u32,
        start_fails: bool,
        failure: Option<String>,
        meetings: Vec<BrowserMeeting>,
    }

    /// A fake browser extension that only records the instructions it receives.
    #[derive(Clone, Default)]
    struct FakeBrowser(Arc<Mutex<BrowserState>>);

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
        fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<PathBuf> {
            let mut state = self.0.lock().unwrap();
            anyhow::ensure!(!state.start_fails, "not allowed");
            state.started.push((tab, path.to_owned()));
            // The browser recorded WebM.
            Ok(path.with_extension("webm"))
        }
        fn stop_capture(&self) -> anyhow::Result<()> {
            self.0.lock().unwrap().stopped += 1;
            Ok(())
        }
        fn take_failure(&self) -> Option<String> {
            self.0.lock().unwrap().failure.take()
        }
    }

    fn request(video: VideoTarget) -> RecordingRequest {
        RecordingRequest {
            output_path: "out.mp4".into(),
            video,
            audio_process_id: None,
            render_device_id: None,
            capture_device_id: None,
        }
    }

    fn recorder() -> (LinkedRecorder<FakeRecorder, FakeBrowser>, FakeRecorder, FakeBrowser) {
        let (platform, browser) = (FakeRecorder::default(), FakeBrowser::default());
        (
            LinkedRecorder::new(platform.clone(), Some(browser.clone())),
            platform,
            browser,
        )
    }

    #[test]
    fn tab_recording_is_delegated_to_the_extension() {
        let (mut linked, platform, browser) = recorder();
        let written = linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        assert_eq!(browser.0.lock().unwrap().started, [(TAB, PathBuf::from("out.mp4"))]);
        assert_eq!(
            written,
            PathBuf::from("out.webm"),
            "the file the extension wrote is reported"
        );
        assert!(platform.state().started.is_empty());

        linked.stop().unwrap();
        assert_eq!(browser.0.lock().unwrap().stopped, 1);
        assert_eq!(platform.state().stop_count, 0);
    }

    #[test]
    fn other_recording_is_delegated_to_os_features() {
        for video in [VideoTarget::Window(7), VideoTarget::Display(None), VideoTarget::Blank] {
            let (mut linked, platform, browser) = recorder();
            linked.start(&request(video.clone())).unwrap();

            assert_eq!(platform.state().started.len(), 1, "{video:?}");
            assert!(browser.0.lock().unwrap().started.is_empty());

            linked.stop().unwrap();
            assert_eq!(platform.state().stop_count, 1);
            assert_eq!(browser.0.lock().unwrap().stopped, 0);
        }
    }

    #[test]
    fn does_not_start_another_recording_while_recording() {
        let (mut linked, platform, _) = recorder();
        linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        assert!(linked.start(&request(VideoTarget::Display(None))).is_err());
        assert!(platform.state().started.is_empty());
    }

    #[test]
    fn stays_in_not_recording_state_when_start_fails() {
        let (mut linked, platform, browser) = recorder();
        browser.0.lock().unwrap().start_fails = true;

        assert!(linked.start(&request(VideoTarget::BrowserTab(TAB))).is_err());

        linked.start(&request(VideoTarget::Display(None))).unwrap();
        assert_eq!(platform.state().started.len(), 1);
    }

    #[test]
    fn tab_cannot_be_recorded_without_extension_connection() {
        let mut linked = LinkedRecorder::<_, FakeBrowser>::new(FakeRecorder::default(), None);

        assert!(linked.start(&request(VideoTarget::BrowserTab(TAB))).is_err());
    }

    #[test]
    fn reports_failures_only_from_the_side_that_is_recording() {
        let (mut linked, platform, browser) = recorder();
        linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();

        platform.fail("unrelated");
        assert_eq!(linked.take_failure(), None);

        browser.0.lock().unwrap().failure = Some("tab closed".into());
        assert_eq!(linked.take_failure(), Some("tab closed".into()));
    }

    #[test]
    fn can_start_next_recording_after_failure_is_reported() {
        let (mut linked, platform, _) = recorder();
        linked.start(&request(VideoTarget::Window(7))).unwrap();
        platform.fail("device lost");

        assert_eq!(linked.take_failure(), Some("device lost".into()));
        linked.start(&request(VideoTarget::Window(7))).unwrap();
    }

    #[test]
    fn window_switching_happens_only_while_recording_through_os_features() {
        let (mut linked, platform, _) = recorder();
        linked.retarget(1).unwrap();
        linked.start(&request(VideoTarget::BrowserTab(TAB))).unwrap();
        linked.retarget(2).unwrap();
        linked.stop().unwrap();
        linked.start(&request(VideoTarget::Window(7))).unwrap();
        linked.retarget(3).unwrap();

        assert_eq!(platform.state().retargets, [3]);
    }

    #[test]
    fn signals_include_meetings_reported_by_the_extension() {
        let (signals, browser) = (ScriptedSignals::default(), FakeBrowser::default());
        signals.set(zoom_in_meeting());
        browser.0.lock().unwrap().meetings = vec![BrowserMeeting {
            tab: TAB,
            service: "GoogleMeet".into(),
            title: None,
        }];

        let snapshot = LinkedSignalSource::new(signals, Some(browser)).capture().unwrap();

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

        let snapshot = LinkedSignalSource::<_, FakeBrowser>::new(signals, None)
            .capture()
            .unwrap();

        assert!(snapshot.browser_links.is_empty() && snapshot.browser_meetings.is_empty());
    }

    #[test]
    fn reports_failure_when_os_signals_cannot_be_obtained() {
        let signals = ScriptedSignals::default();
        signals.set_failure(Some("audio service gone"));

        assert!(
            LinkedSignalSource::new(signals, Some(FakeBrowser::default()))
                .capture()
                .is_err()
        );
    }
}
