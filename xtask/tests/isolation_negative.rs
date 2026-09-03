//! Vacuity guard for the processing-isolation rules and the CI gate declaration.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/violating-workspace/isolation")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ids(workspace: &Path, extra: &[&str]) -> (i32, BTreeSet<String>) {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["boundary", "--workspace"])
        .arg(workspace)
        .args(extra)
        .args(["--format", "json"])
        .output()
        .expect("xtask binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("json expected: {e}: {stdout}"));
    (
        output.status.code().unwrap_or(-1),
        report["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_string())
            .collect(),
    )
}

#[test]
fn isolation_negative_fixture() {
    let (code, found) = ids(&fixture(), &[]);
    assert_ne!(code, 0);
    let expected: BTreeSet<String> = [
        "capture-path:ma-capture->ma-workflow",
        "native-link:ma-engine->fake-whisper-sys",
        "native-link:ma-capture->fake-c-sys",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(found, expected, "one capture-path violation, one declared native binding reached by a non-host crate, one undeclared C-compiling crate reaching the capture path");
    let (_, capture_only) = ids(&fixture(), &["--rule", "capture-path-isolation"]);
    assert_eq!(
        capture_only,
        ["capture-path:ma-capture->ma-workflow".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    let (_, native_only) = ids(&fixture(), &["--rule", "native-inference-confinement"]);
    assert_eq!(
        native_only,
        [
            "native-link:ma-engine->fake-whisper-sys".to_string(),
            "native-link:ma-capture->fake-c-sys".to_string()
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
}

#[test]
fn clean_workspace_passes_both_isolation_rules() {
    for rule in ["capture-path-isolation", "native-inference-confinement"] {
        let (code, found) = ids(&repo_root(), &["--rule", rule]);
        assert_eq!(code, 0, "{rule}: {found:?}");
        assert!(found.is_empty());
    }
}

#[test]
fn ci_defines_portable_and_windows_gates() {
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).unwrap();
    let jobs = ci.split("jobs:").nth(1).expect("jobs section");
    assert!(jobs.contains("\n  portable:"), "portable job is defined");
    assert!(jobs.contains("\n  windows:"), "windows job is defined");
    assert!(
        jobs.contains("runs-on: windows"),
        "windows job runs on a Windows runner"
    );
    assert!(
        ci.contains("phase0-exit-gate"),
        "the windows job is marked as the Phase 0 exit gate"
    );
    assert!(
        ci.contains("cargo xtask verify --tier windows"),
        "the windows job runs the windows tier"
    );
    assert!(
        ci.contains("cargo xtask verify --tier portable"),
        "the portable job runs the portable tier"
    );
    assert!(
        ci.contains("cargo xtask verify --check-registration"),
        "registration is checked in CI"
    );
}
