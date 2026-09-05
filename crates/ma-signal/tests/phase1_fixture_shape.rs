//! contract-replayable-timeline-fixtures: the five Phase 1 fixtures keep the existing
//! header-plus-JSONL shape, carry no real service identifier, and each has a confirmation-label
//! sidecar in the existing shape.
//!
//! Synthetic identifier mapping (the real identifiers observed on the recording host live only in
//! the Windows-tier manual record and in the L4 adapter crates' own fixture lists):
//!
//! | fixture | synthetic image / package | synthetic pid(s) | synthetic host |
//! | --- | --- | --- | --- |
//! | teams-desktop-session | example-desk.exe / ExamplePublisher.Desk_8wekyb3d8bbwe | 4100 | — |
//! | slack-huddle-session | example-other.exe | 4200 | — |
//! | zoom-desktop-session | example-desk-c.exe | 4300 | — |
//! | meet-chrome-with-extension | example-browser.exe | 6300 (tree root), 6301 (helper) | meet.example.test |
//! | meet-chrome-without-extension | example-browser.exe | 6300, 6301 | — |

use ma_signal::{SignalTimeline, TimelineHeader};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const PHASE1_FIXTURES: &[&str] = &[
    "teams-desktop-session",
    "slack-huddle-session",
    "zoom-desktop-session",
    "meet-chrome-with-extension",
    "meet-chrome-without-extension",
];

/// Every pid a Phase 1 fixture may carry.
const SYNTHETIC_PIDS: &[u32] = &[4100, 4200, 4300, 6300, 6301];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(
        repo_root()
            .join("fixtures/signal-timelines")
            .join(format!("{name}.jsonl")),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn windows_fixture_header_matches_timeline_header_shape() {
    for name in PHASE1_FIXTURES {
        let text = fixture_text(name);
        let header_line = text.lines().next().expect("header line");
        let raw: serde_json::Value = serde_json::from_str(header_line).unwrap();
        let keys: BTreeSet<&str> = raw
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "schema_version",
                "adapter_table_version",
                "machine_profile",
                "created"
            ]
            .into_iter()
            .collect(),
            "{name}: the header carries exactly TimelineHeader's fields"
        );
        let header: TimelineHeader = serde_json::from_str(header_line).unwrap();
        assert_eq!(header.schema_version, ma_signal::SCHEMA_VERSION);
        assert_eq!(header.machine_profile, "redacted", "{name}");
        let timeline = SignalTimeline::from_jsonl(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!timeline.signals().is_empty(), "{name}");
        // Round trip: parsing then serialising yields the same lines (the existing JSONL shape).
        let back = SignalTimeline::from_jsonl(&timeline.to_jsonl()).unwrap();
        assert_eq!(back.signals(), timeline.signals(), "{name}");
        assert!(
            timeline
                .signals()
                .windows(2)
                .all(|w| w[0].observed_at.monotonic_ns <= w[1].observed_at.monotonic_ns),
            "{name}: monotonic order"
        );
    }
}

#[test]
fn windows_fixtures_carry_no_real_host_identifiers() {
    // The real identifiers are allowed to live only in the L4 adapter tables: read them from disk
    // (not through a crate edge) and assert none appears in a committed fixture.
    let mut real: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("crates"))
        .unwrap()
        .flatten()
    {
        let table = entry.path().join("adapter.toml");
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("ma-adapter-")
            || !table.is_file()
        {
            continue;
        }
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(table).unwrap()).unwrap();
        for key in [
            "process_images",
            "package_family_names",
            "browser_images",
            "tab_hosts",
        ] {
            if let Some(list) = value.get(key).and_then(|v| v.as_array()) {
                real.extend(
                    list.iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_lowercase),
                );
            }
        }
        if let Some(list) = value
            .get("fixtures")
            .and_then(|f| f.get("positive_hosts"))
            .and_then(|v| v.as_array())
        {
            real.extend(
                list.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_lowercase),
            );
        }
    }
    assert!(
        real.len() >= 8,
        "the adapter tables declare real identifiers"
    );
    for name in PHASE1_FIXTURES {
        let text = fixture_text(name);
        let dir = repo_root().join("fixtures/signal-timelines");
        let labels = std::fs::read_to_string(dir.join(format!("{name}.labels.json"))).unwrap();
        let decisions =
            std::fs::read_to_string(dir.join(format!("{name}.decisions.json"))).unwrap();
        for (what, body) in [
            ("timeline", &text),
            ("labels", &labels),
            ("decisions", &decisions),
        ] {
            let lower = body.to_lowercase();
            for id in &real {
                assert!(
                    !lower.contains(id),
                    "{name} {what} carries the real identifier {id}"
                );
            }
        }
        let timeline = SignalTimeline::from_jsonl(&text).unwrap();
        for signal in timeline.signals() {
            match &signal.subject {
                ma_signal::Subject::Process { pid, .. } => assert!(
                    SYNTHETIC_PIDS.contains(pid),
                    "{name}: pid {pid} is not in the documented synthetic mapping"
                ),
                ma_signal::Subject::Tab { host, .. } => {
                    assert!(host.ends_with(".example.test"), "{name}: host {host}")
                }
                _ => {}
            }
            if let Some(root) = signal.payload.process_tree_root_pid {
                assert!(SYNTHETIC_PIDS.contains(&root), "{name}: tree root {root}");
            }
        }
    }
}

#[test]
fn confirmation_label_matches_labels_json_shape() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/signal-timelines/browser-tab-with-mic.labels.json"
    ))
    .unwrap();
    let reference_keys: BTreeSet<String> = reference["labels"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for name in PHASE1_FIXTURES {
        let path = repo_root()
            .join("fixtures/signal-timelines")
            .join(format!("{name}.labels.json"));
        let sidecar: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}")),
        )
        .unwrap();
        assert_eq!(sidecar["timeline"], format!("{name}.jsonl"));
        let labels = sidecar["labels"].as_array().unwrap();
        assert!(
            !labels.is_empty(),
            "{name}: at least one confirmation label"
        );
        let timeline = SignalTimeline::from_jsonl(&fixture_text(name)).unwrap();
        let last = timeline.signals().last().unwrap().observed_at.monotonic_ns;
        for label in labels {
            let keys: BTreeSet<String> = label.as_object().unwrap().keys().cloned().collect();
            assert_eq!(keys, reference_keys, "{name}: the existing sidecar shape");
            assert!(label["was_meeting"].is_boolean());
            let from = label["from_monotonic_ns"].as_u64().unwrap();
            let to = label["to_monotonic_ns"].as_u64().unwrap();
            assert!(
                from <= to && to <= last,
                "{name}: label range lies within the timeline"
            );
        }
        assert!(
            labels
                .iter()
                .any(|l| l["was_meeting"].as_bool() == Some(true)),
            "{name}: carries a was_meeting confirmation"
        );
    }
}
