//! The signal envelope. Every field is a closed enum, a number, an identifier or an
//! operating-system fact; `contracts/signal/signal-envelope.schema.json` is its JSON Schema.

use ma_core_types::SignalId;
use serde::{Deserialize, Serialize};

/// Envelope schema version carried by every signal and by fixture headers.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    ProcessStarted,
    ProcessStopped,
    PackageIdentityObserved,
    AudioSessionCreated,
    AudioSessionDestroyed,
    MicCaptureStarted,
    MicCaptureStopped,
    AudioActivity,
    TabMeetingPresent,
    TabAudible,
    CalendarEventActive,
    UserCommand,
    SystemSuspend,
    SystemResume,
    CollectorStarted,
}

/// Who vouches for the signal. Extension evidence alone never yields a determinate start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Os,
    Extension,
    User,
    Calendar,
}

/// The closed subject union. `tab.host` is a hostname, never a full URL; `tab_key` is opaque.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Subject {
    Process {
        pid: u32,
        image_name: String,
        package_family_name: Option<String>,
    },
    Device {
        endpoint_id: String,
    },
    Tab {
        host: String,
        tab_key: String,
    },
    System,
}

impl Subject {
    /// A stable key identifying the subject across signals.
    pub fn key(&self) -> String {
        match self {
            Subject::Process {
                pid, image_name, ..
            } => format!("process:{pid}:{image_name}"),
            Subject::Device { endpoint_id } => format!("device:{endpoint_id}"),
            Subject::Tab { host, tab_key } => format!("tab:{host}:{tab_key}"),
            Subject::System => "system".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserCommand {
    Start,
    Stop,
    Pause,
    Resume,
    Cancel,
    Discard,
}

/// Closed payload: every member is typed; there is no free-text field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Payload {
    /// The collector started while the condition was already true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart_resync: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_dbfs: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<UserCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_event_key: Option<String>,
    /// The process tree root of a browser process, so tab and microphone facts can be joined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_tree_root_pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObservedAt {
    /// Ordering clock: monotonic nanoseconds. Never reordered by wall-clock steps.
    pub monotonic_ns: u64,
    /// Human display and correlation only.
    pub wall_utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Signal {
    pub signal_id: SignalId,
    pub source_id: String,
    pub kind: SignalKind,
    pub subject: Subject,
    pub observed_at: ObservedAt,
    #[serde(default)]
    pub payload: Payload,
    pub authority: Authority,
    pub schema_version: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core_types::id::TypedId;
    use std::path::Path;

    fn schema() -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/signal/signal-envelope.schema.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("schema file")).unwrap()
    }

    fn start_sequence() -> Vec<Signal> {
        let subject = Subject::Process {
            pid: 4242,
            image_name: "example-meetings.exe".into(),
            package_family_name: Some("ExamplePublisher.Meetings_8wekyb3d8bbwe".into()),
        };
        let mut t = 1_000_000u64;
        [
            SignalKind::ProcessStarted,
            SignalKind::PackageIdentityObserved,
            SignalKind::AudioSessionCreated,
            SignalKind::MicCaptureStarted,
        ]
        .into_iter()
        .map(|kind| {
            t += 1_000_000;
            Signal {
                signal_id: SignalId::new(),
                source_id: "os.process".into(),
                kind,
                subject: subject.clone(),
                observed_at: ObservedAt {
                    monotonic_ns: t,
                    wall_utc_ms: 1_756_857_600_000 + (t / 1_000_000) as i64,
                },
                payload: Payload::default(),
                authority: Authority::Os,
                schema_version: SCHEMA_VERSION,
            }
        })
        .collect()
    }

    #[test]
    fn schema_golden_roundtrip() {
        let validator = jsonschema::validator_for(&schema()).expect("schema compiles");
        for signal in start_sequence() {
            let json = serde_json::to_value(&signal).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&json)
                .map(|e| e.to_string())
                .collect();
            assert!(
                errors.is_empty(),
                "signal must validate against the JSON Schema: {errors:?}"
            );
            let back: Signal = serde_json::from_value(json).unwrap();
            assert_eq!(back, signal, "JSON round-trip must be lossless");
        }
        // an envelope carrying a UI-derived field is rejected by the schema
        let mut leaked = serde_json::to_value(&start_sequence()[0]).unwrap();
        leaked["subject"]["window_title"] = serde_json::Value::String("Weekly sync".into());
        assert!(
            !validator.is_valid(&leaked),
            "a window title has nowhere to live"
        );
    }

    #[test]
    fn schema_contains_no_free_text_subject() {
        let schema = schema();
        let forbidden = [
            "title",
            "window_title",
            "label",
            "text",
            "url",
            "href",
            "aria",
            "xpath",
            "selector",
            "dom",
            "coordinate",
            "x",
            "y",
            "accessibility",
        ];
        fn walk(
            value: &serde_json::Value,
            path: &str,
            forbidden: &[&str],
            found: &mut Vec<String>,
            unconstrained: &mut Vec<String>,
        ) {
            match value {
                serde_json::Value::Object(map) => {
                    if let Some(serde_json::Value::Object(props)) = map.get("properties") {
                        for (name, prop) in props {
                            if forbidden.contains(&name.as_str()) {
                                found.push(format!("{path}.{name}"));
                            }
                            let is_string = prop.get("type").and_then(|t| t.as_str())
                                == Some("string")
                                || prop
                                    .get("type")
                                    .and_then(|t| t.as_array())
                                    .is_some_and(|a| a.iter().any(|v| v == "string"));
                            if is_string
                                && !(prop.get("enum").is_some()
                                    || prop.get("pattern").is_some()
                                    || prop.get("format").is_some())
                            {
                                unconstrained.push(format!("{path}.{name}"));
                            }
                            walk(
                                prop,
                                &format!("{path}.{name}"),
                                forbidden,
                                found,
                                unconstrained,
                            );
                        }
                    }
                    for (k, v) in map {
                        if k != "properties" {
                            walk(v, &format!("{path}/{k}"), forbidden, found, unconstrained);
                        }
                    }
                }
                serde_json::Value::Array(items) => items
                    .iter()
                    .for_each(|v| walk(v, path, forbidden, found, unconstrained)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        let mut unconstrained = Vec::new();
        walk(&schema, "$", &forbidden, &mut found, &mut unconstrained);
        assert!(
            found.is_empty(),
            "UI-derived field names present: {found:?}"
        );
        assert!(unconstrained.is_empty(), "every string property must be an enum, pattern or format so free text cannot enter: {unconstrained:?}");
        let subjects = &schema["$defs"]["subject"]["oneOf"];
        assert!(
            subjects.as_array().is_some_and(|v| v.len() == 4),
            "subject is a closed union of four variants"
        );
        for variant in subjects.as_array().unwrap() {
            assert_eq!(
                variant["additionalProperties"],
                serde_json::Value::Bool(false)
            );
        }
    }
}
