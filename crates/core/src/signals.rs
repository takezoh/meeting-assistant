use crate::metadata::MeetingMetadata;

/// An audio session with an active stream. Shows which process uses which device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSession {
    pub process_id: u32,
    pub device_id: String,
}

/// One entry in the process list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub process_id: u32,
    pub parent_process_id: u32,
    /// Executable name. Whether the extension is present and letter case are absorbed at evaluation time.
    pub name: String,
}

/// A visible top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEntry {
    /// The window identifier. An OS-specific value that [`crate::Recorder`] interprets as is.
    pub id: u64,
    pub process_id: u32,
    pub title: String,
    /// Identifier of the display holding the window. Format is OS-specific; [`crate::Recorder`] interprets it as is.
    pub display: Option<String>,
    pub area: u64,
}

/// One connection to the browser extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLink {
    pub id: u64,
    /// The relay process the browser launched for the extension. Walking up its parents leads to the browser itself.
    pub host_process_id: u32,
}

/// A browser tab, identified by which tab of which connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BrowserTab {
    pub link: u64,
    pub tab: u64,
}

/// A meeting in progress reported by the browser extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserMeeting {
    pub tab: BrowserTab,
    /// Name of the meeting service. Used in recording file names and logs.
    pub service: String,
    /// What the extension tells of the meeting, which differs per service: a Google Meet meeting code and the tab's
    /// title, or the tab's title.
    pub metadata: MeetingMetadata,
}

/// The full set of signals observed from the OS at a point in time, used for meeting detection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignalSnapshot {
    /// Sessions using an input device such as the microphone.
    pub capture_sessions: Vec<AudioSession>,
    /// Sessions using an output device such as the speakers.
    pub render_sessions: Vec<AudioSession>,
    pub processes: Vec<ProcessEntry>,
    pub windows: Vec<WindowEntry>,
    /// Connected browser extensions.
    pub browser_links: Vec<BrowserLink>,
    /// A meeting in progress reported by the browser extension.
    pub browser_meetings: Vec<BrowserMeeting>,
}

/// A port that acquires signals from the OS. Each kind of signal is a separate read, so that detection reads only
/// what it needs: most of the time no app uses the microphone, and then nothing but the microphone sessions and the
/// extension's reports is read ([`crate::Engine`] decides).
pub trait SignalSource {
    /// Sessions using an input device such as the microphone.
    fn capture_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>>;
    /// Sessions using an output device such as the speakers.
    fn render_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>>;
    fn processes(&mut self) -> anyhow::Result<Vec<ProcessEntry>>;
    fn windows(&mut self) -> anyhow::Result<Vec<WindowEntry>>;
    /// Connected browser extensions and the meetings they report. The OS itself reports none.
    fn browser(&mut self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        (Vec::new(), Vec::new())
    }
}

/// Every signal a source can read, for diagnostics. Detection reads less.
pub fn full_snapshot(source: &mut impl SignalSource) -> anyhow::Result<SignalSnapshot> {
    let (browser_links, browser_meetings) = source.browser();
    Ok(SignalSnapshot {
        capture_sessions: source.capture_sessions()?,
        render_sessions: source.render_sessions()?,
        processes: source.processes()?,
        windows: source.windows()?,
        browser_links,
        browser_meetings,
    })
}
