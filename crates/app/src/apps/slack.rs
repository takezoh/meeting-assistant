//! Slack (desktop).

use meeting_assistant_core::{MeetingApp, MeetingMetadata, WindowRole};

/// What Slack adds to its main window's title while a huddle it has joined is open in another window.
///
/// Observed on macOS on 2026-11-19 with Slack in English: the main window was titled "dev-gcp-errors (Channel) - tkz -
/// Slack" alone, and "… - Slack [Main] 🏠" beside the huddle window "dev-gcp-errors - tkz - Slack" from the moment the
/// huddle was joined, not while it was only previewed. The mark went when the huddle window closed. Slack opens the
/// huddle window as it takes the microphone, so when did a window appear cannot tell the two apart.
const MAIN_MARK: &str = "[Main]";

/// While a window carries [`MAIN_MARK`], it is Slack's main window and the others are the huddle's; otherwise nothing
/// is told. A huddle window titled "<conversation> - <workspace> - Slack" tells both.
#[derive(Debug)]
pub struct Slack;

impl MeetingApp for Slack {
    fn handles(&self, process_name: &str) -> bool {
        process_name == "slack"
    }

    fn window_roles(&self, titles: &[&str]) -> Vec<WindowRole> {
        let marked = titles.iter().any(|title| title.contains(MAIN_MARK));
        titles
            .iter()
            .map(|title| match (marked, title.contains(MAIN_MARK)) {
                (false, _) => WindowRole::Unknown,
                (true, true) => WindowRole::Main,
                (true, false) => WindowRole::Meeting,
            })
            .collect()
    }

    fn meeting_metadata(&self, title: &str) -> MeetingMetadata {
        if title.contains(MAIN_MARK) {
            return MeetingMetadata::default();
        }
        // From the right, since a conversation's name may itself hold " - ".
        let mut parts = title.rsplitn(3, " - ");
        match (parts.next(), parts.next(), parts.next()) {
            (Some("Slack"), Some(workspace), Some(conversation)) => {
                MeetingMetadata::new([("conversation", conversation), ("workspace", workspace)])
            }
            _ => MeetingMetadata::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beside_the_marked_main_window_the_other_window_is_the_huddles() {
        let roles = Slack.window_roles(&[
            "dev-gcp-errors (Channel) - tkz - Slack [Main] 🏠",
            "dev-gcp-errors - tkz - Slack",
        ]);
        assert_eq!(roles, [WindowRole::Main, WindowRole::Meeting]);
    }

    #[test]
    fn without_the_mark_nothing_is_told() {
        assert_eq!(
            Slack.window_roles(&["dev-gcp-errors (Channel) - tkz - Slack", "dev-gcp-errors - tkz - Slack"]),
            [WindowRole::Unknown, WindowRole::Unknown],
            "a previewed huddle"
        );
        assert_eq!(Slack.window_roles(&[]), []);
    }

    #[test]
    fn handles_slack_only() {
        assert!(Slack.handles("slack"));
        assert!(!Slack.handles("zoom"));
    }

    #[test]
    fn a_huddle_window_tells_its_conversation_and_workspace() {
        let told = Slack.meeting_metadata("dev-gcp-errors - tkz - Slack");
        let pairs: Vec<(&str, &str)> = told.entries().iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(pairs, [("conversation", "dev-gcp-errors"), ("workspace", "tkz")]);
        assert_eq!(
            Slack.meeting_metadata("Jane Doe - design - tkz - Slack").label(),
            Some("Jane Doe - design"),
            "a conversation's name may hold the separator"
        );
    }

    #[test]
    fn the_main_window_and_other_titles_tell_nothing() {
        assert!(
            Slack
                .meeting_metadata("dev-gcp-errors (Channel) - tkz - Slack [Main] 🏠")
                .is_empty()
        );
        assert!(Slack.meeting_metadata("Preferences").is_empty());
        assert!(Slack.meeting_metadata("a - b - Not Slack").is_empty());
    }
}
