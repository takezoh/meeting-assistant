//! How recording file names are chosen. No side effects.

use crate::ports::LocalDateTime;

/// File extension of recording files.
pub const RECORDING_EXTENSION: &str = "mp4";

/// Builds a file name without extension from a date/time and a label (such as a rule name).
pub fn recording_stem(time: LocalDateTime, label: &str) -> String {
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}_{}",
        time.year,
        time.month,
        time.day,
        time.hour,
        time.minute,
        time.second,
        sanitize_label(label)
    )
}

/// The `attempt`-th candidate (starting at 1) when the same name already exists.
/// The first has no sequence number; the second and later get `_2`, `_3`, ...
pub fn candidate_file_name(stem: &str, attempt: u32) -> String {
    candidate_file_name_with_extension(stem, attempt, RECORDING_EXTENSION)
}

/// Like [`candidate_file_name`], for a file whose container has another extension.
pub fn candidate_file_name_with_extension(stem: &str, attempt: u32, extension: &str) -> String {
    if attempt <= 1 {
        format!("{stem}.{extension}")
    } else {
        format!("{stem}_{attempt}.{extension}")
    }
}

/// Returns the first candidate that is not yet used. `is_taken` answers whether a name is in use.
pub fn first_unused_file_name(stem: &str, is_taken: impl Fn(&str) -> bool) -> String {
    first_unused_file_name_with_extension(stem, RECORDING_EXTENSION, is_taken)
}

/// Like [`first_unused_file_name`], for a file whose container has another extension.
pub fn first_unused_file_name_with_extension(stem: &str, extension: &str, is_taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|attempt| candidate_file_name_with_extension(stem, attempt, extension))
        .find(|name| !is_taken(name))
        .expect("candidates never run out")
}

/// Turns a label into a form usable as a file name on any OS.
pub fn sanitize_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if c.is_control() || c.is_whitespace() || r#"<>:"/\|?*"#.contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "Meeting".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIME: LocalDateTime = LocalDateTime {
        year: 2026,
        month: 1,
        day: 5,
        hour: 9,
        minute: 3,
        second: 7,
        utc_offset_minutes: 540,
    };

    #[test]
    fn timestamp_fields_are_zero_padded() {
        assert_eq!(recording_stem(TIME, "Zoom"), "2026-01-05_09-03-07_Zoom");
    }

    #[test]
    fn replaces_characters_and_whitespace_not_allowed_in_file_names() {
        assert_eq!(sanitize_label("Web: Meet/Zoom"), "Web--Meet-Zoom");
        assert_eq!(sanitize_label("a\tb\\c|d?e*f<g>h\"i"), "a-b-c-d-e-f-g-h-i");
        assert_eq!(sanitize_label("réunion"), "réunion");
    }

    #[test]
    fn empty_label_uses_the_default_name() {
        assert_eq!(sanitize_label(""), "Meeting");
    }

    #[test]
    fn first_candidate_has_no_sequence_number() {
        assert_eq!(candidate_file_name("a", 1), "a.mp4");
        assert_eq!(candidate_file_name("a", 2), "a_2.mp4");
        assert_eq!(candidate_file_name("a", 3), "a_3.mp4");
        assert_eq!(candidate_file_name_with_extension("a", 1, "webm"), "a.webm");
        assert_eq!(candidate_file_name_with_extension("a", 2, "webm"), "a_2.webm");
    }

    #[test]
    fn picks_the_first_unused_candidate() {
        assert_eq!(first_unused_file_name("a", |_| false), "a.mp4");
        assert_eq!(first_unused_file_name("a", |name| name == "a.mp4"), "a_2.mp4");
        assert_eq!(
            first_unused_file_name("a", |name| ["a.mp4", "a_2.mp4"].contains(&name)),
            "a_3.mp4"
        );
        assert_eq!(
            first_unused_file_name_with_extension("a", "webm", |name| name == "a.webm"),
            "a_2.webm"
        );
    }
}
