//! What a recording records, and the windows the user can choose from to record one by hand. Pure.

use crate::detector::MeetingKey;
use crate::signals::{ProcessEntry, WindowEntry};

/// What a recording records. An automatic recording always records a meeting; the user can also record the desktop or
/// one window by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingTarget {
    /// One detected meeting.
    Meeting(MeetingKey),
    /// The main display and all output audio, with no meeting attached.
    Desktop {
        /// Whether the microphone is recorded into its track; when not, the track is silent.
        microphone: bool,
    },
    /// One window and the sound of its app alone, with no meeting attached.
    Window {
        window: WindowChoice,
        /// Whether the microphone is recorded into its track; when not, the track is silent.
        microphone: bool,
    },
}

impl RecordingTarget {
    /// The meeting recorded, if this is a meeting.
    pub fn meeting(&self) -> Option<&MeetingKey> {
        match self {
            Self::Meeting(key) => Some(key),
            Self::Desktop { .. } | Self::Window { .. } => None,
        }
    }

    /// Whether the microphone is recorded. A meeting's is, unless the configuration turns it off.
    pub fn microphone(&self) -> bool {
        match self {
            Self::Meeting(_) => true,
            Self::Desktop { microphone } | Self::Window { microphone, .. } => *microphone,
        }
    }
}

/// A window the user can choose to record by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowChoice {
    /// The window identifier, as [`WindowEntry::id`].
    pub id: u64,
    /// The process that owns the window; its app's sound is recorded with it.
    pub process_id: u32,
    /// The app's name: its executable's, without an `.exe` extension.
    pub app: String,
    /// The window's title when it was listed. Empty where the system does not tell it.
    pub title: String,
}

/// The windows the user can choose from to record by hand, in the order the system lists them: those it shows as
/// windows of their own, apart from the windows of `own_process` (the app itself) and windows whose process has gone.
pub fn window_choices(
    windows: &[WindowEntry],
    processes: &[ProcessEntry],
    own_process: Option<u32>,
) -> Vec<WindowChoice> {
    windows
        .iter()
        .filter(|window| window.pickable && Some(window.process_id) != own_process)
        .filter_map(|window| {
            let process = processes.iter().find(|p| p.process_id == window.process_id)?;
            Some(WindowChoice {
                id: window.id,
                process_id: window.process_id,
                app: app_name(&process.name),
                title: window.title.clone(),
            })
        })
        .collect()
}

fn app_name(executable: &str) -> String {
    let length = executable.len();
    if length > 4 && executable.is_char_boundary(length - 4) && executable[length - 4..].eq_ignore_ascii_case(".exe") {
        executable[..length - 4].to_owned()
    } else {
        executable.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, process_id: u32, title: &str, pickable: bool) -> WindowEntry {
        WindowEntry {
            id,
            process_id,
            title: title.into(),
            display: None,
            area: 100,
            pickable,
        }
    }

    fn process(process_id: u32, name: &str) -> ProcessEntry {
        ProcessEntry {
            process_id,
            parent_process_id: 1,
            name: name.into(),
        }
    }

    #[test]
    fn the_windows_to_choose_from_are_the_shown_ones_of_other_apps_named_by_their_app() {
        let windows = [
            window(1, 10, "Untitled - Notepad", true),
            window(2, 20, "Windows Input Experience", false),
            window(3, 30, "Meeting Assistant", true),
            window(4, 40, "Downloads", true),
            window(5, 50, "", true),
            window(6, 99, "Gone", true),
        ];
        let processes = [
            process(10, "Notepad.exe"),
            process(20, "TextInputHost.exe"),
            process(30, "meeting-assistant.exe"),
            process(40, "explorer.EXE"),
            process(50, "Slack"),
        ];

        let choices = window_choices(&windows, &processes, Some(30));

        let listed: Vec<_> = choices
            .iter()
            .map(|c| (c.id, c.app.as_str(), c.title.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                (1, "Notepad", "Untitled - Notepad"),
                (4, "explorer", "Downloads"),
                (5, "Slack", "")
            ],
            "in the system's order, without hidden windows, the app's own or those of a process that has gone"
        );
        assert_eq!(choices[0].process_id, 10);
    }

    #[test]
    fn only_a_trailing_exe_is_taken_off_an_apps_name() {
        assert_eq!(app_name("Teams.exe"), "Teams");
        assert_eq!(app_name("Google Chrome"), "Google Chrome");
        assert_eq!(app_name(".exe"), ".exe");
        assert_eq!(app_name("exe"), "exe");
        assert_eq!(app_name("日本語.exe"), "日本語");
    }

    #[test]
    fn only_a_meeting_names_a_meeting_and_only_a_recording_by_hand_can_leave_the_microphone_out() {
        let key = MeetingKey {
            rule_name: "Zoom".into(),
            browser_tab: None,
        };
        assert_eq!(RecordingTarget::Meeting(key.clone()).meeting(), Some(&key));
        assert!(RecordingTarget::Meeting(key).microphone());
        let desktop = RecordingTarget::Desktop { microphone: false };
        assert_eq!(desktop.meeting(), None);
        assert!(!desktop.microphone());
    }
}
