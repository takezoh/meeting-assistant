//! What the status panel and the tray icon show, and what the panel's buttons ask for.
//!
//! Pure: it turns the coordinator's status into texts and flags, and the panel's messages into worker commands.
//! The window, the web view and the clock are the caller's.

use std::path::Path;
use std::time::Duration;

use meeting_assistant_core::{CoordinatorStatus, ManualTarget, MeetingKey, MeetingStatus, RecordingTrigger};
use serde::{Deserialize, Serialize};

use crate::worker::Command;

const PAGE: &str = include_str!("panel.html");

/// Whose design the page follows, so that the panel looks like the platform's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "each platform builds the variant of its own design")]
pub enum Design {
    MacOs,
    Windows,
}

/// The user's accent color, as the platform uses it on a light and on a dark background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    pub light: [u8; 3],
    pub dark: [u8; 3],
}

/// How the platform draws the panel's window, which the page adapts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    pub design: Design,
    /// The window shows the system's translucent material (the macOS popover material, the Windows acrylic of
    /// flyouts) behind a see-through page.
    pub translucent: bool,
    /// The system rounds the window's corners; otherwise the page draws a rounded edge on a transparent window.
    pub rounded_by_system: bool,
    /// The user's accent color, where the page should follow it; otherwise the page uses the platform's default.
    pub accent: Option<Accent>,
}

impl Surface {
    /// Whether the page lets the window show through: over the system material, or around its own rounded edge.
    /// The window must then be transparent too.
    pub fn see_through(self) -> bool {
        self.translucent || !self.rounded_by_system
    }
}

/// The panel's page, marked for the surface it is shown on.
pub fn page(surface: Surface) -> String {
    let design = match surface.design {
        Design::MacOs => "macos",
        Design::Windows => "windows",
    };
    let material = if surface.translucent { "material" } else { "solid" };
    let corners = if surface.rounded_by_system { "window" } else { "css" };
    let hex = |[r, g, b]: [u8; 3]| format!("#{r:02x}{g:02x}{b:02x}");
    let accent = surface.accent.map_or_else(String::new, |accent| {
        format!(
            r#" style="--accent-light: {}; --accent-dark: {}""#,
            hex(accent.light),
            hex(accent.dark)
        )
    });
    PAGE.replacen(
        r#"data-surface="solid""#,
        &format!(r#"data-design="{design}" data-surface="{material}" data-corners="{corners}"{accent}"#),
        1,
    )
}

/// The overall state, shown by the tray icon and at the top of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    Idle,
    /// At least one meeting is detected and nothing is being recorded.
    Meeting,
    Recording,
}

pub fn activity(status: &CoordinatorStatus) -> Activity {
    if status.is_recording() {
        Activity::Recording
    } else if status.meetings.is_empty() {
        Activity::Idle
    } else {
        Activity::Meeting
    }
}

/// Everything the panel shows, sent to the page as JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelView {
    pub activity: Activity,
    pub headline: String,
    pub detail: String,
    pub recording: Option<RecordingView>,
    pub meetings: Vec<MeetingView>,
    pub auto_enabled: bool,
    /// Whether this platform can record the whole screen; when not, the panel does not offer it.
    pub screen_capture: bool,
    /// Whether a recording of the whole screen can be started now.
    pub can_record_screen: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingView {
    /// The meeting's name, or "Entire screen".
    pub title: String,
    pub file_name: String,
    /// "Automatic" or "Manual".
    pub trigger: String,
    pub elapsed_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingView {
    /// Identifies the meeting in the panel's messages.
    pub id: String,
    pub name: String,
    /// What tells it apart from other meetings of the same service, such as a Google Meet code.
    pub title: Option<String>,
    /// How the meeting was detected.
    pub source: String,
    pub state: MeetingState,
    pub note: String,
    pub can_record: bool,
    pub can_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MeetingState {
    Recording,
    /// Recorded automatically when the meeting being recorded automatically ends; one recording at a time.
    Waiting,
    /// Not recorded, and automatic recording will not start it.
    NotRecording,
}

/// Builds the panel's content. `elapsed` is how long the current recording has been running.
pub fn panel_view(status: &CoordinatorStatus, elapsed: Duration, screen_capture: bool) -> PanelView {
    let recording = status.recording.as_ref().map(|recording| RecordingView {
        title: recording
            .meeting
            .as_ref()
            .map_or_else(|| "Entire screen".to_owned(), |key| meeting_label(status, key)),
        file_name: file_name(&recording.path),
        trigger: match recording.trigger {
            RecordingTrigger::Auto => "Automatic",
            RecordingTrigger::Manual => "Manual",
        }
        .to_owned(),
        elapsed_seconds: elapsed.as_secs(),
    });
    let headline = match activity(status) {
        Activity::Recording => "Recording".to_owned(),
        Activity::Meeting if status.meetings.len() == 1 => "Meeting detected".to_owned(),
        Activity::Meeting => format!("{} meetings detected", status.meetings.len()),
        Activity::Idle => "No meeting".to_owned(),
    };
    let detail = match (&recording, status.auto_enabled) {
        (Some(recording), _) => format!("{} · {}", recording.title, recording.trigger),
        (None, true) if status.meetings.is_empty() => "Waiting for a call to start".to_owned(),
        (None, true) => "Not recording".to_owned(),
        (None, false) => "Automatic recording is paused".to_owned(),
    };
    PanelView {
        activity: activity(status),
        headline,
        detail,
        recording,
        meetings: status
            .meetings
            .iter()
            .map(|meeting| meeting_view(status, meeting))
            .collect(),
        auto_enabled: status.auto_enabled,
        screen_capture,
        can_record_screen: screen_capture && !status.is_recording(),
    }
}

fn meeting_view(status: &CoordinatorStatus, meeting: &MeetingStatus) -> MeetingView {
    let recorded = status.is_recording_meeting(&meeting.key);
    let (state, note) = if recorded {
        let trigger = status.recording.as_ref().map(|r| r.trigger);
        let note = if trigger == Some(RecordingTrigger::Manual) {
            "Recording · started by you"
        } else {
            "Recording"
        };
        (MeetingState::Recording, note)
    } else if !status.auto_enabled {
        (MeetingState::NotRecording, "Auto-recording paused")
    } else if meeting.auto_stopped {
        (MeetingState::NotRecording, "Stopped for this meeting")
    } else if status
        .recording
        .as_ref()
        .is_some_and(|r| r.trigger == RecordingTrigger::Auto && r.meeting.is_some())
    {
        // Recorded when the meeting being recorded automatically ends. A recording started or stopped by hand
        // passes nothing on.
        (MeetingState::Waiting, "Waiting to record")
    } else {
        (MeetingState::NotRecording, "Not recording")
    };
    MeetingView {
        id: meeting_id(&meeting.key),
        name: display_name(&meeting.key.rule_name),
        title: meeting.title.clone(),
        // Reported by the extension, or detected from an app's use of the microphone.
        source: if meeting.key.browser_tab.is_some() {
            "Tab"
        } else {
            "App"
        }
        .to_owned(),
        state,
        note: note.to_owned(),
        can_record: !status.is_recording(),
        can_stop: recorded,
    }
}

/// A stable identifier of a meeting for the page.
pub fn meeting_id(key: &MeetingKey) -> String {
    match key.browser_tab {
        Some(tab) => format!("{}@{}:{}", key.rule_name, tab.link, tab.tab),
        None => key.rule_name.clone(),
    }
}

/// A meeting's name with what tells it apart, for one-line places: "Google Meet (abc-defg-hij)". A meeting that
/// is no longer listed is named by its rule alone.
pub fn meeting_label(status: &CoordinatorStatus, key: &MeetingKey) -> String {
    let name = display_name(&key.rule_name);
    match status
        .meetings
        .iter()
        .find(|m| m.key == *key)
        .and_then(|m| m.title.as_deref())
    {
        Some(title) => format!("{name} ({title})"),
        None => name,
    }
}

/// A readable name for a rule or a service reported by the extension. Unknown names are shown as they are.
pub fn display_name(rule_name: &str) -> String {
    match rule_name {
        "GoogleMeet" => "Google Meet",
        "Teams" | "TeamsWeb" => "Microsoft Teams",
        "ZoomWeb" => "Zoom",
        "WebexWeb" => "Webex",
        "SlackWeb" => "Slack",
        "Jitsi" => "Jitsi Meet",
        "BrowserMeeting" => "Meeting in a browser",
        other => other,
    }
    .to_owned()
}

/// A message from the panel's page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PanelMessage {
    /// The page has loaded and can be given content.
    Ready,
    /// The content's height in CSS pixels.
    Resize {
        height: f64,
    },
    Record {
        meeting: String,
    },
    RecordScreen,
    Stop,
    SetAuto {
        enabled: bool,
    },
    OpenRecordings,
    OpenConfig,
    OpenLogs,
    Quit,
    Close,
}

pub fn parse_message(text: &str) -> Option<PanelMessage> {
    serde_json::from_str(text).ok()
}

/// The worker command a message asks for, if any. A meeting that is no longer listed yields nothing.
pub fn command_for(message: &PanelMessage, status: &CoordinatorStatus) -> Option<Command> {
    match message {
        PanelMessage::Record { meeting } => status
            .meetings
            .iter()
            .find(|m| meeting_id(&m.key) == *meeting)
            .map(|m| Command::StartManual(ManualTarget::Meeting(m.key.clone()))),
        PanelMessage::RecordScreen => Some(Command::StartManual(ManualTarget::Screen)),
        PanelMessage::Stop => Some(Command::StopManual),
        PanelMessage::SetAuto { enabled } => Some(Command::SetAutoEnabled(*enabled)),
        _ => None,
    }
}

/// What the tray icon and its menu show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySummary {
    pub activity: Activity,
    /// The first, disabled line of the menu.
    pub status_line: String,
    pub tooltip: String,
    /// The menu item that starts or stops a recording, and what it does.
    pub record_label: String,
    pub record_command: Option<RecordCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordCommand {
    Start(ManualTarget),
    Stop,
}

impl RecordCommand {
    pub fn into_command(self) -> Command {
        match self {
            Self::Start(target) => Command::StartManual(target),
            Self::Stop => Command::StopManual,
        }
    }
}

pub fn tray_summary(status: &CoordinatorStatus, screen_capture: bool) -> TraySummary {
    let names = || {
        status
            .meetings
            .iter()
            .map(|m| meeting_label(status, &m.key))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let status_line = match (&status.recording, status.meetings.len()) {
        (Some(recording), _) => match &recording.meeting {
            Some(key) => format!("Recording: {}", meeting_label(status, key)),
            None => "Recording: entire screen".to_owned(),
        },
        (None, 0) => "No meeting".to_owned(),
        (None, _) => format!("Not recording: {}", names()),
    };
    let first = status.meetings.first();
    let (record_label, record_command) = match (status.is_recording(), first) {
        (true, _) => ("Stop recording".to_owned(), Some(RecordCommand::Stop)),
        (false, Some(meeting)) => (
            format!("Record {}", meeting_label(status, &meeting.key)),
            Some(RecordCommand::Start(ManualTarget::Meeting(meeting.key.clone()))),
        ),
        (false, None) if screen_capture => (
            "Record entire screen".to_owned(),
            Some(RecordCommand::Start(ManualTarget::Screen)),
        ),
        (false, None) => ("Start recording".to_owned(), None),
    };
    TraySummary {
        activity: activity(status),
        tooltip: format!("Meeting Assistant - {status_line}"),
        status_line,
        record_label,
        record_command,
    }
}

/// How the tray icon shows the state on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayLook {
    /// The color of the idle ring, chosen to read on the taskbar. `None` draws it as a template that the system tints
    /// like the rest of the menu bar.
    pub idle_color: Option<[u8; 3]>,
    /// Text can be shown next to the icon (the macOS menu bar). Otherwise the recording's running time goes in the
    /// tooltip.
    pub text_beside_icon: bool,
}

/// The tooltip while recording, with the running time added where it is not shown next to the icon.
pub fn tooltip_while_recording(tooltip: &str, elapsed: Duration) -> String {
    format!("{tooltip} ({})", elapsed_label(elapsed))
}

/// How long the current recording has been running, followed from the statuses the UI receives. Time is passed in.
#[derive(Debug, Default)]
pub struct RecordingClock {
    since: Option<(std::path::PathBuf, std::time::Instant)>,
}

impl RecordingClock {
    /// Takes the file being recorded now: another file starts the clock again, the same file keeps it running,
    /// and no recording stops it.
    pub fn update(&mut self, recording: Option<&Path>, now: std::time::Instant) {
        self.since = match (recording, self.since.take()) {
            (Some(path), Some((seen, since))) if seen == path => Some((seen, since)),
            (Some(path), _) => Some((path.to_owned(), now)),
            (None, _) => None,
        };
    }

    pub fn is_running(&self) -> bool {
        self.since.is_some()
    }

    pub fn elapsed(&self, now: std::time::Instant) -> Duration {
        self.since
            .as_ref()
            .map_or(Duration::ZERO, |(_, since)| now.saturating_duration_since(*since))
    }
}

/// How long until the shown running time turns over to the next whole second.
pub fn until_next_second(elapsed: Duration) -> Duration {
    Duration::from_secs(1) - Duration::from_nanos(u64::from(elapsed.subsec_nanos()))
}

/// A recording's running time as `m:ss`, or `h:mm:ss` from one hour on.
pub fn elapsed_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A rectangle in screen coordinates; all values in the same unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where the panel's top-left corner goes so that it hangs from the tray icon `anchor`, inside `screen`.
///
/// Below the icon when the icon is in the upper half of the screen (the macOS menu bar, a taskbar at the top),
/// otherwise above it, and horizontally centered on the icon as far as the screen allows. `gap` is kept between
/// the panel and the icon and between the panel and the screen's edges.
pub fn panel_origin(anchor: Rect, panel: (f64, f64), screen: Rect, gap: f64) -> (f64, f64) {
    let (width, height) = panel;
    let clamp = |value: f64, low: f64, high: f64| value.min(high).max(low);
    let below = anchor.y + anchor.height / 2.0 < screen.y + screen.height / 2.0;
    let y = if below {
        anchor.y + anchor.height + gap
    } else {
        anchor.y - gap - height
    };
    let x = anchor.x + anchor.width / 2.0 - width / 2.0;
    (
        clamp(x, screen.x + gap, screen.x + screen.width - width - gap),
        clamp(y, screen.y + gap, screen.y + screen.height - height - gap),
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use meeting_assistant_core::{BrowserTab, RecordingStatus};

    use super::*;

    fn key(rule: &str) -> MeetingKey {
        MeetingKey {
            rule_name: rule.into(),
            browser_tab: None,
        }
    }

    fn tab_key(service: &str, tab: u64) -> MeetingKey {
        MeetingKey {
            rule_name: service.into(),
            browser_tab: Some(BrowserTab { link: 3, tab }),
        }
    }

    fn meeting(key: MeetingKey) -> MeetingStatus {
        MeetingStatus {
            key,
            title: None,
            auto_stopped: false,
        }
    }

    fn idle() -> CoordinatorStatus {
        CoordinatorStatus {
            auto_enabled: true,
            meetings: Vec::new(),
            recording: None,
        }
    }

    fn recording(meeting: Option<MeetingKey>, trigger: RecordingTrigger) -> Option<RecordingStatus> {
        Some(RecordingStatus {
            path: "Videos/2026-11-18_14-00-05_Zoom.mp4".into(),
            trigger,
            meeting,
        })
    }

    /// Zoom being recorded automatically while a Google Meet tab is also detected.
    fn two_meetings() -> CoordinatorStatus {
        CoordinatorStatus {
            meetings: vec![meeting(key("Zoom")), meeting(tab_key("GoogleMeet", 9))],
            recording: recording(Some(key("Zoom")), RecordingTrigger::Auto),
            ..idle()
        }
    }

    #[test]
    fn an_idle_status_offers_the_screen_only_where_it_can_be_recorded() {
        let view = panel_view(&idle(), Duration::ZERO, true);
        assert_eq!(view.activity, Activity::Idle);
        assert_eq!(view.headline, "No meeting");
        assert!(view.meetings.is_empty() && view.recording.is_none());
        assert!(view.can_record_screen);

        let view = panel_view(&idle(), Duration::ZERO, false);
        assert!(!view.can_record_screen && !view.screen_capture);
    }

    #[test]
    fn each_detected_meeting_is_listed_with_what_can_be_done_with_it() {
        let view = panel_view(&two_meetings(), Duration::from_secs(75), true);

        assert_eq!(view.activity, Activity::Recording);
        let recording = view.recording.as_ref().unwrap();
        assert_eq!(recording.title, "Zoom");
        assert_eq!(recording.file_name, "2026-11-18_14-00-05_Zoom.mp4");
        assert_eq!(recording.trigger, "Automatic");
        assert_eq!(recording.elapsed_seconds, 75);

        let [zoom, meet] = &view.meetings[..] else { panic!() };
        assert_eq!(
            (zoom.state, zoom.can_stop, zoom.can_record),
            (MeetingState::Recording, true, false)
        );
        assert_eq!(zoom.source, "App");
        assert_eq!(meet.name, "Google Meet");
        assert_eq!(meet.source, "Tab");
        assert_eq!(
            (meet.state, meet.can_stop, meet.can_record),
            (MeetingState::Waiting, false, false)
        );
        assert!(!view.can_record_screen, "one recording at a time");
    }

    #[test]
    fn a_meeting_that_is_not_recorded_can_be_recorded_and_says_why_it_is_not() {
        let mut status = CoordinatorStatus {
            meetings: vec![MeetingStatus {
                key: key("Zoom"),
                title: None,
                auto_stopped: true,
            }],
            ..idle()
        };
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.headline, "Meeting detected");
        assert_eq!(view.meetings[0].state, MeetingState::NotRecording);
        assert!(view.meetings[0].can_record);
        assert_eq!(view.meetings[0].note, "Stopped for this meeting");

        status.auto_enabled = false;
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.meetings[0].note, "Auto-recording paused");
        assert_eq!(view.detail, "Automatic recording is paused");
    }

    /// Seen on a Mac on 2026-11-18: after Stop, automatic recording moved to the other meeting, and the stopped
    /// meeting was shown as waiting although it will not be recorded automatically.
    #[test]
    fn only_a_meeting_that_will_be_recorded_automatically_is_shown_as_waiting() {
        let mut status = CoordinatorStatus {
            meetings: vec![
                MeetingStatus {
                    key: tab_key("GoogleMeet", 1),
                    title: None,
                    auto_stopped: true,
                },
                meeting(tab_key("Whereby", 2)),
            ],
            recording: recording(Some(tab_key("Whereby", 2)), RecordingTrigger::Auto),
            ..idle()
        };
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.meetings[0].state, MeetingState::NotRecording);
        assert_eq!(view.meetings[0].note, "Stopped for this meeting");
        assert!(!view.meetings[0].can_record, "one recording at a time");

        status.meetings[0].auto_stopped = false;
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.meetings[0].state, MeetingState::Waiting);

        // A recording started by hand passes nothing on when it is stopped.
        status.recording = recording(Some(tab_key("Whereby", 2)), RecordingTrigger::Manual);
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.meetings[0].state, MeetingState::NotRecording);
        assert_eq!(view.meetings[0].note, "Not recording");

        status.auto_enabled = false;
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.meetings[0].note, "Auto-recording paused");
    }

    #[test]
    fn two_meetings_of_one_service_are_told_apart_by_their_titles() {
        let titled = |tab, title: &str| MeetingStatus {
            key: tab_key("GoogleMeet", tab),
            title: Some(title.into()),
            auto_stopped: false,
        };
        let status = CoordinatorStatus {
            meetings: vec![titled(1, "abc-defg-hij"), titled(2, "xyz-uvwx-rst")],
            recording: recording(Some(tab_key("GoogleMeet", 1)), RecordingTrigger::Auto),
            ..idle()
        };

        let view = panel_view(&status, Duration::ZERO, true);
        let titles: Vec<_> = view.meetings.iter().map(|m| m.title.as_deref()).collect();
        assert_eq!(titles, [Some("abc-defg-hij"), Some("xyz-uvwx-rst")]);
        assert_eq!(view.recording.unwrap().title, "Google Meet (abc-defg-hij)");

        let summary = tray_summary(&status, true);
        assert_eq!(summary.status_line, "Recording: Google Meet (abc-defg-hij)");
        let waiting = CoordinatorStatus {
            recording: None,
            ..status
        };
        assert_eq!(
            tray_summary(&waiting, true).status_line,
            "Not recording: Google Meet (abc-defg-hij), Google Meet (xyz-uvwx-rst)"
        );
    }

    #[test]
    fn a_screen_recording_is_titled_as_such() {
        let status = CoordinatorStatus {
            recording: recording(None, RecordingTrigger::Manual),
            ..idle()
        };
        let view = panel_view(&status, Duration::ZERO, true);
        assert_eq!(view.recording.unwrap().title, "Entire screen");
        assert_eq!(view.detail, "Entire screen · Manual");
    }

    #[test]
    fn the_view_is_sent_to_the_page_in_camel_case() {
        let json = serde_json::to_value(panel_view(&two_meetings(), Duration::ZERO, true)).unwrap();
        assert_eq!(json["activity"], "recording");
        assert_eq!(json["autoEnabled"], true);
        assert_eq!(json["recording"]["elapsedSeconds"], 0);
        assert_eq!(json["meetings"][1]["id"], "GoogleMeet@3:9");
        assert_eq!(json["meetings"][1]["state"], "waiting");
    }

    #[test]
    fn page_messages_become_worker_commands() {
        let status = two_meetings();
        let command = |text: &str| command_for(&parse_message(text).unwrap(), &status);

        assert!(matches!(
            command(r#"{"type":"record","meeting":"GoogleMeet@3:9"}"#),
            Some(Command::StartManual(ManualTarget::Meeting(k))) if k == tab_key("GoogleMeet", 9)
        ));
        assert!(matches!(
            command(r#"{"type":"recordScreen"}"#),
            Some(Command::StartManual(ManualTarget::Screen))
        ));
        assert!(matches!(command(r#"{"type":"stop"}"#), Some(Command::StopManual)));
        assert!(matches!(
            command(r#"{"type":"setAuto","enabled":false}"#),
            Some(Command::SetAutoEnabled(false))
        ));
        assert!(command(r#"{"type":"openLogs"}"#).is_none());
    }

    #[test]
    fn a_record_message_for_a_meeting_that_is_gone_does_nothing() {
        let message = parse_message(r#"{"type":"record","meeting":"Teams"}"#).unwrap();
        assert!(command_for(&message, &two_meetings()).is_none());
    }

    #[test]
    fn malformed_messages_are_rejected() {
        assert_eq!(parse_message("not json"), None);
        assert_eq!(parse_message(r#"{"type":"format-disk"}"#), None);
        assert_eq!(parse_message(r#"{"type":"record"}"#), None);
        assert_eq!(
            parse_message(r#"{"type":"resize","height":312.5}"#),
            Some(PanelMessage::Resize { height: 312.5 })
        );
    }

    #[test]
    fn the_tray_menu_records_the_first_meeting_or_the_screen_and_stops_a_recording() {
        let summary = tray_summary(&two_meetings(), true);
        assert_eq!(summary.activity, Activity::Recording);
        assert_eq!(summary.status_line, "Recording: Zoom");
        assert_eq!(summary.tooltip, "Meeting Assistant - Recording: Zoom");
        assert_eq!(summary.record_command, Some(RecordCommand::Stop));

        let waiting = CoordinatorStatus {
            recording: None,
            ..two_meetings()
        };
        let summary = tray_summary(&waiting, true);
        assert_eq!(summary.status_line, "Not recording: Zoom, Google Meet");
        assert_eq!(summary.record_label, "Record Zoom");
        assert_eq!(
            summary.record_command,
            Some(RecordCommand::Start(ManualTarget::Meeting(key("Zoom"))))
        );

        assert_eq!(
            tray_summary(&idle(), true).record_command,
            Some(RecordCommand::Start(ManualTarget::Screen))
        );
        assert_eq!(tray_summary(&idle(), false).record_command, None);
    }

    #[test]
    fn the_recording_clock_keeps_running_for_the_same_file_and_restarts_for_another() {
        let start = std::time::Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut clock = RecordingClock::default();
        assert!(!clock.is_running());

        clock.update(Some(Path::new("a.mp4")), at(0));
        clock.update(Some(Path::new("a.mp4")), at(5));
        assert_eq!(
            clock.elapsed(at(12)),
            Duration::from_secs(12),
            "the same file keeps its start"
        );

        clock.update(Some(Path::new("b.mp4")), at(20));
        assert_eq!(
            clock.elapsed(at(21)),
            Duration::from_secs(1),
            "another file starts again"
        );

        clock.update(None, at(30));
        assert!(!clock.is_running());
        assert_eq!(clock.elapsed(at(31)), Duration::ZERO);
    }

    #[test]
    fn the_running_time_turns_over_on_whole_seconds() {
        assert_eq!(
            until_next_second(Duration::from_millis(12_250)),
            Duration::from_millis(750)
        );
        assert_eq!(until_next_second(Duration::from_secs(3)), Duration::from_secs(1));
    }

    #[test]
    fn elapsed_time_is_shown_in_minutes_and_hours() {
        assert_eq!(elapsed_label(Duration::from_secs(0)), "0:00");
        assert_eq!(elapsed_label(Duration::from_secs(754)), "12:34");
        assert_eq!(elapsed_label(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn the_page_is_marked_for_the_surface_it_is_shown_on() {
        let page = page(Surface {
            design: Design::MacOs,
            translucent: true,
            rounded_by_system: false,
            accent: None,
        });
        assert!(page.contains(r#"<html lang="en" data-design="macos" data-surface="material" data-corners="css">"#));
        // Everything the page needs is inside it; it loads nothing from elsewhere.
        assert!(page.contains("default-src 'none'"));
        assert!(!page.contains("src=\"http") && !page.contains("href=\"http"));
    }

    #[test]
    fn the_page_follows_the_windows_design_and_the_users_accent_color() {
        let page = page(Surface {
            design: Design::Windows,
            translucent: false,
            rounded_by_system: true,
            accent: Some(Accent {
                light: [0, 95, 184],
                dark: [96, 205, 255],
            }),
        });
        assert!(page.contains(
            r#"data-design="windows" data-surface="solid" data-corners="window" style="--accent-light: #005fb8; --accent-dark: #60cdff">"#
        ));
    }

    #[test]
    fn the_tooltip_carries_the_running_time_where_it_is_not_beside_the_icon() {
        let summary = tray_summary(&two_meetings(), true);
        assert_eq!(
            tooltip_while_recording(&summary.tooltip, Duration::from_secs(754)),
            "Meeting Assistant - Recording: Zoom (12:34)"
        );
    }

    #[test]
    fn known_services_get_readable_names() {
        assert_eq!(display_name("TeamsWeb"), "Microsoft Teams");
        assert_eq!(display_name("BrowserMeeting"), "Meeting in a browser");
        assert_eq!(display_name("My call app"), "My call app");
    }

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    #[test]
    fn the_panel_hangs_below_an_icon_in_the_menu_bar() {
        let icon = Rect {
            x: 1500.0,
            y: 0.0,
            width: 30.0,
            height: 24.0,
        };
        assert_eq!(panel_origin(icon, (360.0, 400.0), SCREEN, 8.0), (1335.0, 32.0));
    }

    #[test]
    fn the_panel_rises_above_an_icon_in_a_bottom_taskbar_and_stays_on_screen() {
        let icon = Rect {
            x: 1890.0,
            y: 1040.0,
            width: 24.0,
            height: 40.0,
        };
        assert_eq!(panel_origin(icon, (360.0, 400.0), SCREEN, 8.0), (1552.0, 632.0));
    }

    #[test]
    fn a_panel_taller_than_the_room_left_is_kept_inside_the_screen() {
        let icon = Rect {
            x: 100.0,
            y: 0.0,
            width: 30.0,
            height: 24.0,
        };
        let (x, y) = panel_origin(icon, (360.0, 1200.0), SCREEN, 8.0);
        assert_eq!(x, 8.0);
        assert!(y >= 8.0);
    }
}
