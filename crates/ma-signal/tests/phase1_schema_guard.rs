//! contract-closed-schema-discipline: Phase 1's collectors, capture measurements and extension
//! path added no field, variant or free-text value to the closed signal schema. This file adds no
//! production code path; it only asserts the absence of drift.

use ma_core_types::id::TypedId;
use ma_core_types::SignalId;
use ma_signal::{
    Authority, ObservedAt, Payload, Signal, SignalKind, SignalTimeline, Subject, UserCommand,
    SCHEMA_VERSION,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const PHASE1_FIXTURES: &[&str] = &[
    "teams-desktop-session",
    "slack-huddle-session",
    "zoom-desktop-session",
    "meet-chrome-with-extension",
    "meet-chrome-without-extension",
];

/// The frozen Payload field set. Adding a field fails here before it reaches a fixture.
const PAYLOAD_FIELDS: &[&str] = &[
    "restart_resync",
    "audible",
    "level_dbfs",
    "command",
    "calendar_event_key",
    "process_tree_root_pid",
];

/// The frozen Subject variant set (the `type` tag values).
const SUBJECT_VARIANTS: &[&str] = &["process", "device", "tab", "system"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn schema() -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("contracts/signal/signal-envelope.schema.json"))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn windows_fixtures_conform_to_signal_schema() {
    let validator = jsonschema::validator_for(&schema()).expect("schema compiles");
    let mut checked = 0;
    for name in PHASE1_FIXTURES {
        let text = std::fs::read_to_string(
            repo_root()
                .join("fixtures/signal-timelines")
                .join(format!("{name}.jsonl")),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        for (i, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let json: serde_json::Value = serde_json::from_str(line).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&json)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{name} line {}: {errors:?}", i + 1);
            // The typed parse agrees with the schema and round-trips losslessly (an absent
            // payload and an empty one are the same closed value).
            let signal: Signal = serde_json::from_value(json.clone()).unwrap();
            let again: Signal =
                serde_json::from_value(serde_json::to_value(&signal).unwrap()).unwrap();
            assert_eq!(again, signal, "{name} line {}", i + 1);
            assert!(
                validator.is_valid(&serde_json::to_value(&signal).unwrap()),
                "{name} line {}: re-serialised signal validates",
                i + 1
            );
            checked += 1;
        }
        // The timeline as a whole still parses through the existing reader.
        SignalTimeline::from_jsonl(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert!(
        checked >= 40,
        "the corpus is not trivially small: {checked} signals"
    );
}

#[test]
fn payload_and_subject_field_sets_are_unchanged() {
    // A fully populated payload serialises to exactly the frozen field set.
    let full = Payload {
        restart_resync: true,
        audible: Some(true),
        level_dbfs: Some(-20),
        command: Some(UserCommand::Start),
        calendar_event_key: Some("cal-1".into()),
        process_tree_root_pid: Some(6300),
    };
    let json = serde_json::to_value(&full).unwrap();
    let keys: BTreeSet<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, PAYLOAD_FIELDS.iter().copied().collect());
    // The schema agrees with the type.
    let schema = schema();
    let schema_payload: BTreeSet<String> = schema["$defs"]["payload"]["properties"]
        .as_object()
        .expect("payload properties")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        schema_payload,
        PAYLOAD_FIELDS.iter().map(|s| s.to_string()).collect(),
        "schema payload properties are the frozen set"
    );
    // An unknown payload field is rejected by the schema.
    let mut leaked = serde_json::to_value(Payload::default()).unwrap();
    leaked["leak_dbfs"] = serde_json::json!(-30);
    let validator = jsonschema::validator_for(&schema["$defs"]["payload"]).unwrap();
    assert!(!validator.is_valid(&leaked), "payload is closed");

    // Exactly the four Subject variants, each closed.
    let variants = schema["$defs"]["subject"]["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), SUBJECT_VARIANTS.len());
    let tags: BTreeSet<String> = variants
        .iter()
        .map(|v| {
            assert_eq!(v["additionalProperties"], serde_json::Value::Bool(false));
            v["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        tags,
        SUBJECT_VARIANTS.iter().map(|s| s.to_string()).collect()
    );
    let subjects = [
        Subject::Process {
            pid: 1,
            image_name: "example.exe".into(),
            package_family_name: None,
        },
        Subject::Device {
            endpoint_id: "{ep}".into(),
        },
        Subject::Tab {
            host: "meet.example.test".into(),
            tab_key: "tab-1".into(),
        },
        Subject::System,
    ];
    let typed_tags: BTreeSet<String> = subjects
        .iter()
        .map(|s| {
            serde_json::to_value(s).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        typed_tags, tags,
        "the Rust union and the schema union agree"
    );
    // A signal built from the types validates as a whole.
    let signal = Signal {
        signal_id: SignalId::new(),
        source_id: "os.audio_session".into(),
        kind: SignalKind::MicCaptureStarted,
        subject: subjects[0].clone(),
        observed_at: ObservedAt {
            monotonic_ns: 1,
            wall_utc_ms: 1_756_944_000_000,
        },
        payload: full,
        authority: Authority::Os,
        schema_version: SCHEMA_VERSION,
    };
    let whole = jsonschema::validator_for(&schema).unwrap();
    assert!(whole.is_valid(&serde_json::to_value(&signal).unwrap()));
}
