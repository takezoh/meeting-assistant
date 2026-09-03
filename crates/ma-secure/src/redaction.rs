//! Type-level log redaction (contract-diagnostic-redaction). Only identifiers, enum states,
//! counts, durations and typed error codes implement [`LogValue`]; meeting content is a
//! [`Content`] that implements neither `LogValue` nor `Display`, so leaking it requires a
//! deliberate unwrap at the call site.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Meeting text: transcript, summary, title, participant name. Not loggable, not displayable,
/// not serializable by a general-purpose serializer.
pub struct Content(String);

impl Content {
    pub fn new(text: impl Into<String>) -> Self {
        Content(text.into())
    }
    /// The only way to read the text; use it where the content is legitimately consumed.
    pub fn unwrap_content(&self) -> &str {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<content {} bytes>", self.0.len())
    }
}

/// A value that may appear in a diagnostic log field.
pub trait LogValue {
    fn log_repr(&self) -> String;
}

macro_rules! log_value_display {
    ($($t:ty),*) => { $(impl LogValue for $t { fn log_repr(&self) -> String { self.to_string() } })* };
}
log_value_display!(u8, u16, u32, u64, usize, i32, i64, bool);
log_value_display!(
    ma_core_types::MeetingId,
    ma_core_types::SessionId,
    ma_core_types::TrackId,
    ma_core_types::ChunkId,
    ma_core_types::ArtifactId,
    ma_core_types::StepId,
    ma_core_types::ExportId,
    ma_core_types::SignalId,
    ma_core_types::DecisionId,
    ma_core_types::RootId
);

impl LogValue for Duration {
    fn log_repr(&self) -> String {
        format!("{}ms", self.as_millis())
    }
}

/// A typed error code: a short static identifier, never a message that echoes input.
impl LogValue for &'static str {
    fn log_repr(&self) -> String {
        (*self).to_string()
    }
}

impl LogValue for RedactedPath {
    fn log_repr(&self) -> String {
        self.0.clone()
    }
}

/// A structured log field: only [`LogValue`]s can be attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogField {
    pub name: &'static str,
    pub value: String,
}

impl LogField {
    pub fn new(name: &'static str, value: &dyn LogValue) -> Self {
        LogField {
            name,
            value: value.log_repr(),
        }
    }
}

/// A path scrubbed to root-relative form for panic hooks and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedPath(String);

impl RedactedPath {
    /// Render `path` relative to `artifact_root` (`<root>/…`); a path outside the root keeps only
    /// its file name.
    pub fn scrub(artifact_root: &Path, path: &Path) -> RedactedPath {
        match path.strip_prefix(artifact_root) {
            Ok(relative) => {
                let mut out = PathBuf::from("<root>");
                for component in relative.components() {
                    if let Component::Normal(part) = component {
                        out.push(part);
                    }
                }
                RedactedPath(out.to_string_lossy().replace('\\', "/"))
            }
            Err(_) => RedactedPath(format!(
                "<outside-root>/{}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )),
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A parse error that reports position and expectation, never the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub category: &'static str,
}

impl ParseError {
    pub fn from_json(err: &serde_json::Error) -> ParseError {
        let category = match err.classify() {
            serde_json::error::Category::Io => "io",
            serde_json::error::Category::Syntax => "syntax",
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "eof",
        };
        ParseError {
            line: err.line(),
            column: err.column(),
            category,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "parse error ({}) at line {} column {}",
            self.category, self.line, self.column
        )
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core_types::id::TypedId;

    #[test]
    fn parse_error_display_elides_payload() {
        let document = r#"{"transcript": "ZZ-SECRET-CONTENT-ZZ", "title": "ZZ-TITLE-ZZ", "key": "ZZ-TOKEN-ZZ", "broken": }"#;
        let raw = serde_json::from_str::<serde_json::Value>(document).unwrap_err();
        assert!(
            raw.to_string().contains("column"),
            "serde reports a position"
        );
        let elided = ParseError::from_json(&raw);
        let shown = elided.to_string();
        for marker in [
            "ZZ-SECRET-CONTENT-ZZ",
            "ZZ-TITLE-ZZ",
            "ZZ-TOKEN-ZZ",
            "transcript",
        ] {
            assert!(
                !shown.contains(marker),
                "{shown} must not echo the document"
            );
        }
        assert!(shown.contains("line 1"));
        assert!(elided.column > 0);
        assert_eq!(elided.category, "syntax");
    }

    #[test]
    fn scrubbed_paths_are_root_relative() {
        let root = Path::new("/home/user/artifacts");
        let inside = RedactedPath::scrub(
            root,
            Path::new("/home/user/artifacts/meetings/abc/chunks/000001.wav"),
        );
        assert_eq!(inside.as_str(), "<root>/meetings/abc/chunks/000001.wav");
        let outside = RedactedPath::scrub(root, Path::new("/home/user/Documents/secret plan.docx"));
        assert_eq!(outside.as_str(), "<outside-root>/secret plan.docx");
    }

    #[test]
    fn log_fields_take_identifiers_states_and_counts() {
        let id = ma_core_types::MeetingId::new();
        let fields = [
            LogField::new("meeting_id", &id),
            LogField::new("chunks", &12u32),
            LogField::new("state", &"recording"),
            LogField::new("elapsed", &Duration::from_millis(1500)),
        ];
        assert_eq!(fields[0].value, id.to_string());
        assert_eq!(fields[3].value, "1500ms");
        let content = Content::new("ZZ-SECRET-CONTENT-ZZ");
        assert_eq!(format!("{content:?}"), "<content 20 bytes>");
    }
}
