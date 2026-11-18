//! State of the connections to the extension and its transitions. Touches no sockets or files.
//!
//! Tracks, per connection, which tabs are in a meeting, and holds at most one tab recording overall.
//! Incoming recording data goes to a [`ChunkSink`]; commands to send to the extension are returned to the caller.

use std::collections::BTreeMap;
use std::io;

use base64::Engine as _;
use meeting_assistant_core::{BrowserLink, BrowserMeeting, BrowserTab};

use super::protocol::{Incoming, Outgoing};

/// Name given to a meeting whose service name was not reported.
const UNNAMED_SERVICE: &str = "BrowserMeeting";

/// Destination for recording data.
pub trait ChunkSink: Send {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
}

/// Progress of a tab recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Start was requested; waiting for the extension to respond.
    Starting,
    Recording,
    /// The extension finished recording and all data, including the last piece, has arrived.
    Stopped,
    Failed(String),
}

impl Phase {
    /// Whether the extension is still recording (or about to start).
    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::Starting | Self::Recording)
    }
}

struct Capture {
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
    /// What tells this meeting apart from others of the same service.
    title: Option<String>,
}

/// Longest title kept for a meeting, in characters. It is only shown; a longer one is cut.
const MAX_TITLE_CHARS: usize = 120;

#[derive(Default)]
pub struct Hub {
    next_link: u64,
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
                    title: meeting.title.clone(),
                })
            })
            .collect();
        (links, meetings)
    }

    /// Starts recording a tab. Returns the command to send to the extension.
    pub fn begin_capture(&mut self, tab: BrowserTab, sink: Box<dyn ChunkSink>) -> anyhow::Result<Outgoing> {
        anyhow::ensure!(self.capture.is_none(), "A tab is already being recorded");
        anyhow::ensure!(
            self.links.contains_key(&tab.link),
            "Not connected to the browser extension"
        );
        self.capture = Some(Capture {
            tab,
            sink,
            phase: Phase::Starting,
            mime: String::new(),
            bytes: 0,
        });
        Ok(Outgoing::Start { tab: tab.tab })
    }

    pub fn capture_phase(&self) -> Option<&Phase> {
        self.capture.as_ref().map(|capture| &capture.phase)
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

    /// Applies a message received from the extension.
    pub fn receive(&mut self, link: u64, message: Incoming) {
        match message {
            Incoming::Hello { .. } | Incoming::Unknown => {}
            Incoming::Meeting {
                tab,
                active,
                service,
                title,
            } => {
                let Some(entry) = self.links.get_mut(&link) else { return };
                if active {
                    let service = if service.is_empty() {
                        UNNAMED_SERVICE.to_owned()
                    } else {
                        service
                    };
                    let title = title.trim();
                    let title = (!title.is_empty()).then(|| title.chars().take(MAX_TITLE_CHARS).collect());
                    entry.meetings.insert(tab, ReportedMeeting { service, title });
                } else {
                    entry.meetings.remove(&tab);
                }
            }
            Incoming::Started { tab, mime } => {
                if let Some(capture) = self.capture_of(link, tab)
                    && capture.phase == Phase::Starting
                {
                    capture.phase = Phase::Recording;
                    capture.mime = mime;
                }
            }
            Incoming::Chunk { tab, data } => {
                let Some(capture) = self.capture_of(link, tab) else {
                    return;
                };
                let written = base64::engine::general_purpose::STANDARD
                    .decode(data.as_bytes())
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        capture
                            .sink
                            .write(&bytes)
                            .map(|()| bytes.len())
                            .map_err(|error| error.to_string())
                    });
                match written {
                    Ok(length) => capture.bytes += length as u64,
                    Err(error) => capture.phase = Phase::Failed(format!("Cannot write recording data: {error}")),
                }
            }
            Incoming::Stopped { tab } => {
                if let Some(capture) = self.capture_of(link, tab)
                    && capture.phase.is_in_progress()
                {
                    capture.phase = Phase::Stopped;
                }
            }
            Incoming::Error { tab, message } => {
                log::error!("The extension reported an error: {message}");
                // An error without a tab is treated as concerning the tab being recorded on that connection.
                let capture = match tab {
                    Some(tab) => self.capture_of(link, tab),
                    None => self.capture.as_mut().filter(|capture| capture.tab.link == link),
                };
                if let Some(capture) = capture
                    && capture.phase.is_in_progress()
                {
                    capture.phase = Phase::Failed(message);
                }
            }
            Incoming::Warning { message } => log::warn!("Warning from the extension: {message}"),
        }
    }

    /// The recording, if the tab being recorded is `tab` on `link`.
    fn capture_of(&mut self, link: u64, tab: u64) -> Option<&mut Capture> {
        self.capture
            .as_mut()
            .filter(|capture| capture.tab == BrowserTab { link, tab })
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

    fn chunk(tab: u64, bytes: &[u8]) -> Incoming {
        Incoming::Chunk {
            tab,
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    fn meeting(tab: u64, active: bool, service: &str) -> Incoming {
        Incoming::Meeting {
            tab,
            active,
            service: service.into(),
            title: String::new(),
        }
    }

    #[test]
    fn keeps_the_title_that_tells_two_meetings_of_a_service_apart() {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        let titled = |tab, title: &str| Incoming::Meeting {
            tab,
            active: true,
            service: "GoogleMeet".into(),
            title: title.into(),
        };
        hub.receive(link, titled(1, "abc-defg-hij"));
        hub.receive(link, titled(2, "  "));
        hub.receive(link, titled(3, &"x".repeat(500)));

        let titles: Vec<_> = hub.snapshot().1.into_iter().map(|m| m.title).collect();
        assert_eq!(titles[0].as_deref(), Some("abc-defg-hij"));
        assert_eq!(titles[1], None, "a blank title is no title");
        assert_eq!(titles[2].as_ref().map(|t| t.chars().count()), Some(MAX_TITLE_CHARS));
    }

    /// State right after one connection exists and recording of tab 7 was requested.
    fn capturing() -> (Hub, u64, MemorySink) {
        let mut hub = Hub::default();
        let link = hub.connect(301);
        let sink = MemorySink::default();
        hub.begin_capture(BrowserTab { link, tab: 7 }, Box::new(sink.clone()))
            .unwrap();
        (hub, link, sink)
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
    fn starting_a_recording_returns_a_command_and_waits_for_response() {
        let mut hub = Hub::default();
        let link = hub.connect(301);

        let instruction = hub
            .begin_capture(BrowserTab { link, tab: 7 }, Box::new(MemorySink::default()))
            .unwrap();

        assert_eq!(instruction, Outgoing::Start { tab: 7 });
        assert_eq!(hub.capture_phase(), Some(&Phase::Starting));
    }

    #[test]
    fn cannot_record_a_tab_on_an_unconnected_link() {
        let mut hub = Hub::default();

        assert!(
            hub.begin_capture(BrowserTab { link: 9, tab: 7 }, Box::new(MemorySink::default()))
                .is_err()
        );
        assert_eq!(hub.capture_phase(), None);
    }

    #[test]
    fn cannot_start_another_recording_while_recording() {
        let (mut hub, link, _) = capturing();

        assert!(
            hub.begin_capture(BrowserTab { link, tab: 8 }, Box::new(MemorySink::default()))
                .is_err()
        );
    }

    #[test]
    fn follows_recording_from_start_response_through_data_to_stop() {
        let (mut hub, link, sink) = capturing();

        hub.receive(
            link,
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into(),
            },
        );
        assert_eq!(hub.capture_phase(), Some(&Phase::Recording));

        hub.receive(link, chunk(7, b"abc"));
        hub.receive(link, chunk(7, b"de"));
        assert_eq!(hub.stop_request(), Some((link, Outgoing::Stop { tab: 7 })));

        hub.receive(link, Incoming::Stopped { tab: 7 });
        assert_eq!(
            hub.stop_request(),
            None,
            "no stop command is issued after it has stopped"
        );

        let finished = hub.end_capture().unwrap();
        assert_eq!(finished.outcome(), Ok(()));
        assert_eq!(finished.bytes, 5);
        assert_eq!(*sink.written.lock().unwrap(), b"abcde");
        assert_eq!(hub.capture_phase(), None);
    }

    #[test]
    fn data_from_another_tab_or_connection_is_not_mixed_in() {
        let (mut hub, link, sink) = capturing();
        let other = hub.connect(401);
        hub.receive(
            link,
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into(),
            },
        );

        hub.receive(link, chunk(8, b"other tab"));
        hub.receive(other, chunk(7, b"other link"));
        hub.receive(other, Incoming::Stopped { tab: 7 });

        assert!(sink.written.lock().unwrap().is_empty());
        assert_eq!(hub.capture_phase(), Some(&Phase::Recording));
    }

    #[test]
    fn extension_error_can_be_taken_as_a_failure() {
        let (mut hub, link, _) = capturing();
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
        let (mut hub, link, _) = capturing();
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
    fn connection_loss_during_recording_is_a_failure() {
        let (mut hub, link, _) = capturing();
        hub.receive(
            link,
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into(),
            },
        );
        hub.disconnect(link);

        assert!(matches!(hub.capture_phase(), Some(Phase::Failed(_))));
    }

    #[test]
    fn connection_loss_after_stopping_is_not_a_failure() {
        let (mut hub, link, _) = capturing();
        hub.receive(
            link,
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into(),
            },
        );
        hub.receive(link, Incoming::Stopped { tab: 7 });
        hub.disconnect(link);

        assert_eq!(hub.capture_phase(), Some(&Phase::Stopped));
    }

    #[test]
    fn write_failure_is_a_failure() {
        let (mut hub, link, sink) = capturing();
        hub.receive(
            link,
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into(),
            },
        );
        *sink.fails.lock().unwrap() = true;
        hub.receive(link, chunk(7, b"abc"));

        let error = hub.take_failed().unwrap().outcome().unwrap_err();
        assert!(error.contains("disk full"), "{error}");
    }

    #[test]
    fn corrupt_data_is_a_failure() {
        let (mut hub, link, _) = capturing();
        hub.receive(
            link,
            Incoming::Chunk {
                tab: 7,
                data: "this is not base64".into(),
            },
        );

        assert!(matches!(hub.capture_phase(), Some(Phase::Failed(_))));
    }

    #[test]
    fn recording_ended_without_a_response_is_reported_as_failed() {
        let (mut hub, _, _) = capturing();

        assert!(hub.end_capture().unwrap().outcome().is_err());
    }

    #[test]
    fn file_extension_is_chosen_from_recording_format_once_recording_has_started() {
        let extension = |mime: &str| {
            let (mut hub, link, _) = capturing();
            assert_eq!(hub.capture_extension(), None, "unknown before the extension says");
            hub.receive(
                link,
                Incoming::Started {
                    tab: 7,
                    mime: mime.into(),
                },
            );
            hub.capture_extension()
        };

        assert_eq!(extension("video/mp4;codecs=avc1.640028,mp4a.40.2"), Some("mp4"));
        assert_eq!(extension("video/webm;codecs=vp9,opus"), Some("webm"));
        assert_eq!(extension(""), Some("mp4"));
    }
}
