//! State of the connections to the extension and its transitions. Touches no sockets or files.
//!
//! Tracks, per connection, which tabs are in a meeting, and holds at most one tab recording overall.
//! Control messages arrive through [`Hub::receive`]. The recording itself arrives on a separate upload connection,
//! whose events the caller passes to [`Hub::upload_opened`], [`Hub::upload_data`] and [`Hub::upload_closed`]; the
//! data goes to a [`ChunkSink`]. Commands to send to the extension are returned to the caller.

use std::collections::BTreeMap;
use std::io;

use meeting_assistant_core::{BrowserLink, BrowserMeeting, BrowserTab, MeetingMetadata};

use super::protocol::{Incoming, Outgoing, PictureSize};

/// Name given to a meeting whose service name was not reported.
const UNNAMED_SERVICE: &str = "BrowserMeeting";

/// Destination for recording data.
pub trait ChunkSink: Send {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
}

/// Progress of a tab recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Start was requested; waiting for the extension to connect its upload.
    Starting,
    Recording,
    /// The extension finished recording and closed its upload after the last piece.
    Stopped,
    Failed(String),
}

impl Phase {
    /// Whether the extension is still recording (or about to start).
    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::Starting | Self::Recording)
    }
}

/// Identifies one recording, so that an upload connection that outlives its recording cannot touch the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureId(u64);

/// Where the extension sends the recording, and the token that proves it was told to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub url: String,
    pub token: String,
}

struct Capture {
    id: CaptureId,
    tab: BrowserTab,
    sink: Box<dyn ChunkSink>,
    phase: Phase,
    mime: String,
    bytes: u64,
}

/// A recording that has finished (or been cut off).
pub struct Finished {
    /// Output destination. Stays open until this value is dropped.
    _sink: Box<dyn ChunkSink>,
    pub phase: Phase,
    /// Format the extension used for recording.
    pub mime: String,
    pub bytes: u64,
}

impl Finished {
    /// Whether the recording arrived intact to the end; if not, the reason.
    pub fn outcome(&self) -> Result<(), String> {
        match &self.phase {
            Phase::Stopped => Ok(()),
            Phase::Failed(message) => Err(message.clone()),
            Phase::Starting | Phase::Recording => Err("The extension did not respond".to_owned()),
        }
    }
}

/// File extension of the container the extension reported (MP4 unless it says WebM).
fn container_extension(mime: &str) -> &'static str {
    if mime.contains("webm") { "webm" } else { "mp4" }
}

#[derive(Default)]
struct Link {
    host_process_id: u32,
    /// Tabs in a meeting.
    meetings: BTreeMap<u64, ReportedMeeting>,
}

struct ReportedMeeting {
    service: String,
    /// What the extension tells of the meeting, kept by the same rules as any app's.
    metadata: MeetingMetadata,
}

#[derive(Default)]
pub struct Hub {
    next_link: u64,
    next_capture: u64,
    links: BTreeMap<u64, Link>,
    capture: Option<Capture>,
}

impl Hub {
    /// Registers a new connection and returns its identifier.
    pub fn connect(&mut self, host_process_id: u32) -> u64 {
        self.next_link += 1;
        self.links.insert(
            self.next_link,
            Link {
                host_process_id,
                meetings: BTreeMap::new(),
            },
        );
        self.next_link
    }

    /// A connection dropped. If a tab on that connection was being recorded, treat it as a failure.
    pub fn disconnect(&mut self, link: u64) {
        self.links.remove(&link);
        if let Some(capture) = &mut self.capture
            && capture.tab.link == link
            && capture.phase.is_in_progress()
        {
            capture.phase = Phase::Failed("Lost the connection to the browser".to_owned());
        }
    }

    /// Connected extensions and the meetings they report.
    pub fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        let links = self
            .links
            .iter()
            .map(|(&id, link)| BrowserLink {
                id,
                host_process_id: link.host_process_id,
            })
            .collect();
        let meetings = self
            .links
            .iter()
            .flat_map(|(&link, entry)| {
                entry.meetings.iter().map(move |(&tab, meeting)| BrowserMeeting {
                    tab: BrowserTab { link, tab },
                    service: meeting.service.clone(),
                    metadata: meeting.metadata.clone(),
                })
            })
            .collect();
        (links, meetings)
    }

    /// Starts recording a tab into `sink`, received at `upload`, at most `max_size`. Returns the recording's identifier
    /// and the command to send to the extension.
    pub fn begin_capture(
        &mut self,
        tab: BrowserTab,
        sink: Box<dyn ChunkSink>,
        upload: Upload,
        max_size: PictureSize,
    ) -> anyhow::Result<(CaptureId, Outgoing)> {
        anyhow::ensure!(self.capture.is_none(), "A tab is already being recorded");
        anyhow::ensure!(
            self.links.contains_key(&tab.link),
            "Not connected to the browser extension"
        );
        self.next_capture += 1;
        let id = CaptureId(self.next_capture);
        self.capture = Some(Capture {
            id,
            tab,
            sink,
            phase: Phase::Starting,
            mime: String::new(),
            bytes: 0,
        });
        Ok((
            id,
            Outgoing::Start {
                tab: tab.tab,
                upload: upload.url,
                token: upload.token,
                max_size,
            },
        ))
    }

    pub fn capture_phase(&self) -> Option<&Phase> {
        self.capture.as_ref().map(|capture| &capture.phase)
    }

    /// Whether `id` is the recording being made and is still waiting for its upload or receiving it.
    pub fn is_capturing(&self, id: CaptureId) -> bool {
        self.capture
            .as_ref()
            .is_some_and(|capture| capture.id == id && capture.phase.is_in_progress())
    }

    /// File extension of the container being recorded, once the extension has said which it is.
    pub fn capture_extension(&self) -> Option<&'static str> {
        let capture = self
            .capture
            .as_ref()
            .filter(|capture| capture.phase != Phase::Starting)?;
        Some(container_extension(&capture.mime))
    }

    /// If the extension is still recording, the stop command and the connection to send it to.
    pub fn stop_request(&self) -> Option<(u64, Outgoing)> {
        let capture = self.capture.as_ref().filter(|capture| capture.phase.is_in_progress())?;
        Some((capture.tab.link, Outgoing::Stop { tab: capture.tab.tab }))
    }

    /// Ends the recording in its current state.
    pub fn end_capture(&mut self) -> Option<Finished> {
        let capture = self.capture.take()?;
        Some(Finished {
            _sink: capture.sink,
            phase: capture.phase,
            mime: capture.mime,
            bytes: capture.bytes,
        })
    }

    /// Takes the recording out if it has failed.
    pub fn take_failed(&mut self) -> Option<Finished> {
        if matches!(self.capture_phase()?, Phase::Failed(_)) {
            self.end_capture()
        } else {
            None
        }
    }

    /// The extension connected its upload for recording `id` and records in the container `mime`.
    pub fn upload_opened(&mut self, id: CaptureId, mime: String) {
        if let Some(capture) = self.capture_with(id)
            && capture.phase == Phase::Starting
        {
            capture.phase = Phase::Recording;
            capture.mime = mime;
        }
    }

    /// The next piece of recording `id`, in order.
    pub fn upload_data(&mut self, id: CaptureId, bytes: &[u8]) {
        let Some(capture) = self.capture_with(id).filter(|c| c.phase == Phase::Recording) else {
            return;
        };
        match capture.sink.write(bytes) {
            Ok(()) => capture.bytes += bytes.len() as u64,
            Err(error) => capture.phase = Phase::Failed(format!("Cannot write recording data: {error}")),
        }
    }

    /// The upload of recording `id` ended: `Ok` after the last piece, otherwise with the reason it broke off.
    pub fn upload_closed(&mut self, id: CaptureId, outcome: Result<(), String>) {
        if let Some(capture) = self.capture_with(id)
            && capture.phase.is_in_progress()
        {
            capture.phase = match outcome {
                Ok(()) if capture.phase == Phase::Recording => Phase::Stopped,
                Ok(()) => Phase::Failed("The extension closed the upload before recording".to_owned()),
                Err(reason) => Phase::Failed(reason),
            };
        }
    }

    /// Applies a control message received from the extension.
    pub fn receive(&mut self, link: u64, message: Incoming) {
        match message {
            Incoming::Hello { .. } => {}
            Incoming::Meeting {
                tab,
                active,
                service,
                metadata,
            } => {
                let Some(entry) = self.links.get_mut(&link) else { return };
                if active {
                    let service = if service.is_empty() {
                        UNNAMED_SERVICE.to_owned()
                    } else {
                        service
                    };
                    let metadata = MeetingMetadata::new(metadata);
                    entry.meetings.insert(tab, ReportedMeeting { service, metadata });
                } else {
                    entry.meetings.remove(&tab);
                }
            }
            Incoming::Error { tab, message } => {
                log::error!("The extension reported an error: {message}");
                // An error without a tab is treated as concerning the tab being recorded on that connection.
                let capture = self
                    .capture
                    .as_mut()
                    .filter(|capture| capture.tab.link == link && tab.is_none_or(|tab| capture.tab.tab == tab));
                if let Some(capture) = capture
                    && capture.phase.is_in_progress()
                {
                    capture.phase = Phase::Failed(message);
                }
            }
        }
    }

    fn capture_with(&mut self, id: CaptureId) -> Option<&mut Capture> {
        self.capture.as_mut().filter(|capture| capture.id == id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A sink that only remembers the data written to it.
    #[derive(Clone, Default)]
    struct MemorySink {
        written: Arc<Mutex<Vec<u8>>>,
        fails: Arc<Mutex<bool>>,
    }

    impl ChunkSink for MemorySink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
            if *self.fails.lock().unwrap() {
                return Err(io::Error::other("disk full"));
            }
            self.written.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
    }

    fn meeting(tab: u64, active: bool, service: &str) -> Incoming {
        Incoming::Meeting {
            tab,
            active,
            service: service.into(),
            metadata: Vec::new(),
        }
    }

    fn upload() -> Upload {
        Upload {
            url: "ws://127.0.0.1:5000/".into(),
            token: "secret".into(),
        }
    }

    const MAX_SIZE: PictureSize = PictureSize {
        width: 1280,
        height: 720,
    };

    /// State right after one connection exists and recording of tab 7 was requested.
    fn capturing() -> (Hub, u64, CaptureId, MemorySink) {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        let sink = MemorySink::default();
        let (id, _) = hub
            .begin_capture(BrowserTab { link, tab: 7 }, Box::new(sink.clone()), upload(), MAX_SIZE)
            .unwrap();
        (hub, link, id, sink)
    }

    /// Recording of tab 7, with its upload connected.
    fn recording() -> (Hub, u64, CaptureId, MemorySink) {
        let (mut hub, link, id, sink) = capturing();
        hub.upload_opened(id, "video/mp4".into());
        (hub, link, id, sink)
    }

    #[test]
    fn keeps_what_the_extension_tells_of_a_meeting_by_the_rules_for_any_app() {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        let told = |tab, metadata: &[(&str, &str)]| Incoming::Meeting {
            tab,
            active: true,
            service: "GoogleMeet".into(),
            metadata: metadata.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        hub.receive(
            link,
            told(
                1,
                &[("meeting_code", "abc-defg-hij"), ("page_title", "Meet - abc-defg-hij")],
            ),
        );
        hub.receive(link, told(2, &[("page_title", "  ")]));
        let long = "x".repeat(500);
        hub.receive(link, told(3, &[("Bad Key", "dropped"), ("page_title", long.as_str())]));

        let meetings = hub.snapshot().1;
        assert_eq!(meetings[0].metadata.label(), Some("abc-defg-hij"));
        assert_eq!(meetings[0].metadata.entries().len(), 2);
        assert!(meetings[1].metadata.is_empty(), "a blank value is dropped");
        assert_eq!(
            meetings[2].metadata.label().map(|label| label.chars().count()),
            Some(meeting_assistant_core::MAX_VALUE_CHARS)
        );
    }

    #[test]
    fn remembers_reported_meetings_per_connection_and_tab() {
        let mut hub = Hub::default();
        let first = hub.connect(301);
        let second = hub.connect(401);
        hub.receive(first, meeting(7, true, "GoogleMeet"));
        hub.receive(second, meeting(7, true, "TeamsWeb"));

        let (links, meetings) = hub.snapshot();
        assert_eq!(
            links.iter().map(|l| (l.id, l.host_process_id)).collect::<Vec<_>>(),
            [(first, 301), (second, 401)]
        );
        assert_eq!(
            meetings.iter().map(|m| (m.tab, m.service.as_str())).collect::<Vec<_>>(),
            [
                (BrowserTab { link: first, tab: 7 }, "GoogleMeet"),
                (BrowserTab { link: second, tab: 7 }, "TeamsWeb")
            ]
        );
    }

    #[test]
    fn forgets_meeting_when_end_is_reported() {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        hub.receive(link, meeting(7, true, "GoogleMeet"));
        hub.receive(link, meeting(7, false, ""));

        assert!(hub.snapshot().1.is_empty());
    }

    #[test]
    fn meeting_without_service_name_gets_default_name() {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        hub.receive(link, meeting(7, true, ""));

        assert_eq!(hub.snapshot().1[0].service, "BrowserMeeting");
    }

    #[test]
    fn dropping_a_connection_removes_its_meetings() {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        hub.receive(link, meeting(7, true, "GoogleMeet"));
        hub.disconnect(link);

        let (links, meetings) = hub.snapshot();
        assert!(links.is_empty() && meetings.is_empty());
    }

    #[test]
    fn starting_a_recording_tells_the_extension_where_to_upload_and_the_largest_picture_and_waits() {
        let mut hub = Hub::default();
        let link = hub.connect(301);

        let (id, instruction) = hub
            .begin_capture(
                BrowserTab { link, tab: 7 },
                Box::new(MemorySink::default()),
                upload(),
                MAX_SIZE,
            )
            .unwrap();

        assert_eq!(
            instruction,
            Outgoing::Start {
                tab: 7,
                upload: "ws://127.0.0.1:5000/".into(),
                token: "secret".into(),
                max_size: MAX_SIZE
            }
        );
        assert_eq!(hub.capture_phase(), Some(&Phase::Starting));
        assert!(hub.is_capturing(id));
    }

    #[test]
    fn cannot_record_a_tab_on_an_unconnected_link() {
        let mut hub = Hub::default();

        assert!(
            hub.begin_capture(
                BrowserTab { link: 9, tab: 7 },
                Box::new(MemorySink::default()),
                upload(),
                MAX_SIZE
            )
            .is_err()
        );
        assert_eq!(hub.capture_phase(), None);
    }

    #[test]
    fn cannot_start_another_recording_while_recording() {
        let (mut hub, link, _, _) = capturing();

        assert!(
            hub.begin_capture(
                BrowserTab { link, tab: 8 },
                Box::new(MemorySink::default()),
                upload(),
                MAX_SIZE
            )
            .is_err()
        );
    }

    #[test]
    fn follows_recording_from_upload_through_data_to_its_close() {
        let (mut hub, link, id, sink) = capturing();

        hub.upload_opened(id, "video/mp4".into());
        assert_eq!(hub.capture_phase(), Some(&Phase::Recording));

        hub.upload_data(id, b"abc");
        hub.upload_data(id, b"de");
        assert_eq!(hub.stop_request(), Some((link, Outgoing::Stop { tab: 7 })));

        hub.upload_closed(id, Ok(()));
        assert_eq!(
            hub.stop_request(),
            None,
            "no stop command is issued after it has stopped"
        );
        assert!(!hub.is_capturing(id));

        let finished = hub.end_capture().unwrap();
        assert_eq!(finished.outcome(), Ok(()));
        assert_eq!(finished.bytes, 5);
        assert_eq!(*sink.written.lock().unwrap(), b"abcde");
        assert_eq!(hub.capture_phase(), None);
    }

    #[test]
    fn an_upload_left_over_from_an_earlier_recording_touches_nothing() {
        let (mut hub, link, earlier, _) = recording();
        hub.end_capture();
        let sink = MemorySink::default();
        let (current, _) = hub
            .begin_capture(BrowserTab { link, tab: 7 }, Box::new(sink.clone()), upload(), MAX_SIZE)
            .unwrap();
        hub.upload_opened(current, "video/mp4".into());

        hub.upload_data(earlier, b"late data");
        hub.upload_closed(earlier, Err("gone".into()));

        assert!(sink.written.lock().unwrap().is_empty());
        assert_eq!(hub.capture_phase(), Some(&Phase::Recording));
        assert!(!hub.is_capturing(earlier));
    }

    #[test]
    fn data_before_the_upload_says_its_format_is_not_written() {
        let (mut hub, _, id, sink) = capturing();
        hub.upload_data(id, b"early");

        assert!(sink.written.lock().unwrap().is_empty());
    }

    #[test]
    fn an_upload_closed_with_a_reason_is_a_failure() {
        let (mut hub, _, id, _) = recording();
        hub.upload_closed(id, Err("Recording failed (NotReadableError)".into()));

        let failed = hub.take_failed().unwrap();
        assert_eq!(failed.outcome(), Err("Recording failed (NotReadableError)".to_owned()));
    }

    #[test]
    fn an_upload_closed_before_it_said_its_format_is_a_failure() {
        let (mut hub, _, id, _) = capturing();
        hub.upload_closed(id, Ok(()));

        assert!(matches!(hub.capture_phase(), Some(Phase::Failed(_))));
    }

    #[test]
    fn extension_error_can_be_taken_as_a_failure() {
        let (mut hub, link, _, _) = capturing();
        hub.receive(
            link,
            Incoming::Error {
                tab: Some(7),
                message: "not allowed".into(),
            },
        );

        let failed = hub.take_failed().unwrap();
        assert_eq!(failed.outcome(), Err("not allowed".to_owned()));
        assert!(hub.take_failed().is_none(), "it can be taken only once");
    }

    #[test]
    fn error_without_tab_fails_the_recording_on_that_connection() {
        let (mut hub, link, _, _) = capturing();
        let other = hub.connect(401);

        hub.receive(
            other,
            Incoming::Error {
                tab: None,
                message: "unrelated".into(),
            },
        );
        assert_eq!(hub.capture_phase(), Some(&Phase::Starting));

        hub.receive(
            link,
            Incoming::Error {
                tab: None,
                message: "broken".into(),
            },
        );
        assert_eq!(hub.capture_phase(), Some(&Phase::Failed("broken".into())));
    }

    #[test]
    fn an_error_about_another_tab_is_not_this_recordings_failure() {
        let (mut hub, link, _, _) = capturing();
        hub.receive(
            link,
            Incoming::Error {
                tab: Some(8),
                message: "other tab".into(),
            },
        );

        assert_eq!(hub.capture_phase(), Some(&Phase::Starting));
    }

    #[test]
    fn connection_loss_during_recording_is_a_failure() {
        let (mut hub, link, _, _) = recording();
        hub.disconnect(link);

        assert!(matches!(hub.capture_phase(), Some(Phase::Failed(_))));
    }

    #[test]
    fn connection_loss_after_stopping_is_not_a_failure() {
        let (mut hub, link, id, _) = recording();
        hub.upload_closed(id, Ok(()));
        hub.disconnect(link);

        assert_eq!(hub.capture_phase(), Some(&Phase::Stopped));
    }

    #[test]
    fn write_failure_is_a_failure() {
        let (mut hub, _, id, sink) = recording();
        *sink.fails.lock().unwrap() = true;
        hub.upload_data(id, b"abc");

        let error = hub.take_failed().unwrap().outcome().unwrap_err();
        assert!(error.contains("disk full"), "{error}");
    }

    #[test]
    fn recording_ended_without_a_response_is_reported_as_failed() {
        let (mut hub, _, _, _) = capturing();

        assert!(hub.end_capture().unwrap().outcome().is_err());
    }

    #[test]
    fn file_extension_is_chosen_from_recording_format_once_recording_has_started() {
        let extension = |mime: &str| {
            let (mut hub, _, id, _) = capturing();
            assert_eq!(hub.capture_extension(), None, "unknown before the extension says");
            hub.upload_opened(id, mime.into());
            hub.capture_extension()
        };

        assert_eq!(extension("video/mp4;codecs=avc1.640028,mp4a.40.2"), Some("mp4"));
        assert_eq!(extension("video/webm;codecs=vp9,opus"), Some("webm"));
        assert_eq!(extension(""), Some("mp4"));
    }
}
