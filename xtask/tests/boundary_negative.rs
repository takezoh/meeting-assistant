//! Vacuity guard for the boundary check: the fixture workspace carries exactly three planted
//! violations and three decoys, and the checker must report the three and none of the decoys.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violating-workspace")
        .join(name)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run_boundary(workspace: &Path, extra: &[&str]) -> (i32, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["boundary", "--workspace"])
        .arg(workspace)
        .args(extra)
        .args(["--format", "json"])
        .output()
        .expect("xtask binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "json report expected, got {e}: {stdout} / {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap_or(-1), report)
}

fn ids(report: &serde_json::Value) -> BTreeSet<String> {
    report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn boundary_negative_fixture_reports_three_violations() {
    let (code, report) = run_boundary(&fixture("boundary"), &[]);
    assert_ne!(
        code, 0,
        "a workspace with planted violations must fail the check"
    );
    let expected: BTreeSet<String> = [
        "edge:ma-workflow->ma-adapter-zoom",
        "literal-b:ma-core-types:src/lib.rs:8",
        "import:ma-detect:src/lib.rs:2",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        ids(&report),
        expected,
        "exactly the planted violations, no decoys: {report}"
    );
}

#[test]
fn feature_gated_adapter_edge_is_detected() {
    let manifest =
        std::fs::read_to_string(fixture("boundary").join("crates/ma-workflow/Cargo.toml")).unwrap();
    assert!(
        manifest.contains("optional = true"),
        "the planted adapter edge must be feature-gated in the fixture"
    );
    let (_, report) = run_boundary(&fixture("boundary"), &[]);
    assert!(
        ids(&report).contains("edge:ma-workflow->ma-adapter-zoom"),
        "feature-gated edge must be resolved with all features: {report}"
    );
    let detail = report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "edge:ma-workflow->ma-adapter-zoom")
        .unwrap()["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        detail.contains("ma-workflow -> ma-adapter-zoom"),
        "the dependency path is named: {detail}"
    );
}

#[test]
fn clean_workspace_reports_edge_count_and_passes() {
    let (code, report) = run_boundary(&repo_root(), &[]);
    assert_eq!(code, 0, "the real workspace must be clean: {report}");
    assert!(report["edges_checked"].is_u64());
    assert!(ids(&report).is_empty());
}

#[test]
fn ci_workflow_invokes_boundary_and_deny() {
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).unwrap();
    assert!(
        ci.contains("cargo xtask boundary"),
        "CI must invoke the boundary check"
    );
    assert!(ci.contains("cargo deny check"), "CI must invoke cargo deny");
}

#[test]
fn check_registration_fails_when_a_t2_id_is_absent() {
    let root = repo_root();
    let original = std::fs::read_to_string(root.join("verification-tiers.toml")).unwrap();
    let removed = "v-tier-windows-suite-green";
    let mut blocks: Vec<&str> = original.split("[[verification]]").collect();
    let header = blocks.remove(0);
    let kept: Vec<&str> = blocks
        .into_iter()
        .filter(|b| !b.contains(&format!("id = \"{removed}\"")))
        .collect();
    assert!(
        kept.len() < original.matches("[[verification]]").count(),
        "fixture removal must drop one registration"
    );
    let mut text = header.to_string();
    for b in kept {
        text.push_str("[[verification]]");
        text.push_str(b);
    }
    let dir = std::env::temp_dir().join(format!("xtask-registration-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tiers = dir.join("verification-tiers.toml");
    std::fs::write(&tiers, text).unwrap();
    let plan = root.join(
        "docs/changes/change-20260903-phase0-repository-and-contracts/design-plan/spine.yaml",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["verify", "--check-registration", "--workspace"])
        .arg(&root)
        .arg("--tiers")
        .arg(&tiers)
        .arg("--plan")
        .arg(&plan)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_ne!(
        output.status.code(),
        Some(0),
        "an unregistered T2 id must fail: {stdout}"
    );
    assert!(
        stdout.contains(removed),
        "the missing id is named: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
