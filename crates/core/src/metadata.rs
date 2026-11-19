//! What is known of a meeting besides its app, and what is written into a recording's file.
//!
//! Each app tells different things about its meetings: a Slack huddle its conversation and workspace, a Google Meet
//! call its meeting code and page title. Whoever knows an app (its adapter, or the browser extension for a browser
//! meeting) gives them as keys and values; the rest of the app only carries them, under the same rules for every app.

use crate::ports::LocalDateTime;

/// The most entries kept.
pub const MAX_ENTRIES: usize = 8;
/// The longest key, in bytes.
pub const MAX_KEY_LEN: usize = 32;
/// The longest value, in characters; a longer one is cut.
pub const MAX_VALUE_CHARS: usize = 200;

/// What is known of a meeting besides its app: keys and values, in the order given. Keys differ per app; the first
/// entry is what tells the meeting apart from others of the same app, and is shown to the user.
///
/// Built only by [`MeetingMetadata::new`], which keeps a key only if it is lowercase ASCII letters, digits and `_`, at
/// most [`MAX_KEY_LEN`] long and not given before; trims each value, drops an empty one and cuts it at
/// [`MAX_VALUE_CHARS`]; and keeps at most [`MAX_ENTRIES`] entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeetingMetadata(Vec<(String, String)>);

impl MeetingMetadata {
    pub fn new<K: Into<String>, V: Into<String>>(entries: impl IntoIterator<Item = (K, V)>) -> Self {
        let mut kept: Vec<(String, String)> = Vec::new();
        for (key, value) in entries {
            let (key, value) = (key.into(), value.into());
            let value = value.trim();
            if kept.len() == MAX_ENTRIES
                || !valid_key(&key)
                || value.is_empty()
                || kept.iter().any(|(existing, _)| *existing == key)
            {
                continue;
            }
            kept.push((key, value.chars().take(MAX_VALUE_CHARS).collect()));
        }
        Self(kept)
    }

    pub fn entries(&self) -> &[(String, String)] {
        &self.0
    }

    /// What tells the meeting apart from others of the same app: the first entry's value.
    pub fn label(&self) -> Option<&str> {
        self.0.first().map(|(_, value)| value.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// What is written into a recording's file besides its media.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingMetadata {
    /// The app the recording is of, as its file is named: a rule's name, a browser service, or `Manual`.
    pub app: String,
    /// When the recording started, the time its file is named after.
    pub started_at: Option<LocalDateTime>,
    /// What is known of the meeting recorded; nothing for a recording of the screen.
    pub meeting: MeetingMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_valid_entries_in_order_and_drops_the_rest() {
        let metadata = MeetingMetadata::new([
            ("meeting_code", " abc-defg-hij "),
            ("Page Title", "bad key"),
            ("page_title", "   "),
            ("meeting_code", "a repeated key"),
            ("page_title", "Meet - abc-defg-hij"),
        ]);
        assert_eq!(
            metadata.entries(),
            [
                ("meeting_code".to_owned(), "abc-defg-hij".to_owned()),
                ("page_title".to_owned(), "Meet - abc-defg-hij".to_owned())
            ]
        );
        assert_eq!(metadata.label(), Some("abc-defg-hij"));
    }

    #[test]
    fn cuts_long_values_and_keeps_a_bounded_number_of_entries() {
        let long = "あ".repeat(MAX_VALUE_CHARS + 10);
        let metadata = MeetingMetadata::new([("title", long.as_str())]);
        assert_eq!(metadata.label().unwrap().chars().count(), MAX_VALUE_CHARS);

        let many = MeetingMetadata::new((0..20).map(|i| (format!("key_{i}"), "value")));
        assert_eq!(many.entries().len(), MAX_ENTRIES);
        assert!(MeetingMetadata::new([("x".repeat(MAX_KEY_LEN + 1), "value")]).is_empty());
    }
}
