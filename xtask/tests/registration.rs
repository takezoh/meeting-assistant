//! contract-windows-tier-verification-registration and contract-manual-verification-record:
//! the registry holds more than one plan, Windows-only code is target-gated, every manual id has a
//! procedure and every procedure names a plan id, and a record is rejected when it is stale,
//! incomplete or not a pass.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Runs xtask against `root` as the workspace (xtask resolves the workspace from its own manifest
/// unless told otherwise, so fixture workspaces must be passed explicitly).
fn xtask(args: &[&str], root: &Path) -> (i32, serde_json::Value, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .arg("--workspace")
        .arg(root)
        .current_dir(root)
        .output()
        .expect("xtask binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let json = serde_json::from_str(stdout.trim()).unwrap_or(serde_json::Value::Null);
    (
        output.status.code().unwrap_or(-1),
        json,
        stdout + &String::from_utf8_lossy(&output.stderr),
    )
}

fn tiers() -> toml::Value {
    toml::from_str(&std::fs::read_to_string(repo_root().join("verification-tiers.toml")).unwrap())
        .unwrap()
}

fn manual_manifest() -> toml::Value {
    toml::from_str(&std::fs::read_to_string(repo_root().join("manual-verification.toml")).unwrap())
        .unwrap()
}

/// Verification ids declared by one spine, with their tiers.
fn plan_ids(spine: &Path) -> Vec<(String, String)> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(spine).unwrap()).unwrap();
    let mut out = Vec::new();
    for contract in doc["contracts"].as_sequence().unwrap() {
        for v in contract["verification"].as_sequence().into_iter().flatten() {
            out.push((
                v["id"].as_str().unwrap().to_string(),
                v["tier"].as_str().unwrap_or("T0").to_string(),
            ));
        }
    }
    out
}

#[test]
fn registration_unions_every_declared_plan() {
    let root = repo_root();
    let file = tiers();
    let plans: Vec<String> = file["plans"]
        .as_array()
        .expect("plans array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        plans.len() >= 2,
        "Phase 0 and Phase 1 spines are both declared"
    );
    assert!(
        plans.contains(&file["plan"].as_str().unwrap().to_string()),
        "the single-plan field is still declared and unioned"
    );
    // The union of every plan's ids is the registered set: check-registration is green on the
    // real repository, so no Phase 0 id is stale and every Phase 1 id is present.
    let (code, report, text) = xtask(
        &["verify", "--check-registration", "--format", "json"],
        &root,
    );
    assert_eq!(code, 0, "{text}");
    assert_eq!(report["ok"], true, "{text}");
    let registered: BTreeSet<String> = file["verification"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    let mut declared = BTreeSet::new();
    for plan in &plans {
        for (id, tier) in plan_ids(&root.join(plan)) {
            assert!(declared.insert(id.clone()), "{id} declared by two plans");
            let reg = file["verification"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"].as_str() == Some(&id))
                .unwrap_or_else(|| panic!("{id} not registered"));
            let expected = if tier == "T2" { "windows" } else { "portable" };
            assert_eq!(reg["tier"].as_str(), Some(expected), "{id}");
            if tier == "T2" {
                assert_eq!(reg["platform"].as_str(), Some("windows"), "{id}");
            }
        }
    }
    assert_eq!(
        declared, registered,
        "registered set == union of declared ids"
    );

    // A registry that repoints the single field at one plan only makes the other plan's ids stale.
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(root.join("verification-tiers.toml")).unwrap();
    let single_plan_only = text
        .lines()
        .filter(|l| !l.starts_with("plans = [") && !l.starts_with("  \"docs/") && *l != "]")
        .collect::<Vec<_>>()
        .join("\n");
    let single = dir.path().join("single.toml");
    std::fs::write(&single, single_plan_only).unwrap();
    let (code, report, _) = xtask(
        &[
            "verify",
            "--check-registration",
            "--tiers",
            single.to_str().unwrap(),
            "--format",
            "json",
        ],
        &root,
    );
    assert_ne!(code, 0);
    let findings = report["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.as_str().unwrap().contains("stale registration")),
        "without the union the second plan's ids are stale: {findings:?}"
    );
    // A duplicate registration is still rejected under the union.
    let dup = text.clone() + "\n[[verification]]\nid = \"v-win1-registration-unions-plans\"\ntier = \"portable\"\nplan_tier = \"T0\"\ncommand = \"cargo test -p xtask registration_unions_every_declared_plan\"\n";
    let dup_path = dir.path().join("dup.toml");
    std::fs::write(&dup_path, dup).unwrap();
    let (code, report, _) = xtask(
        &[
            "verify",
            "--check-registration",
            "--tiers",
            dup_path.to_str().unwrap(),
            "--format",
            "json",
        ],
        &root,
    );
    assert_ne!(code, 0);
    assert!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().contains("registered 2 times")));
}

#[test]
fn windows_only_dependencies_are_target_gated() {
    let root = repo_root();
    let workspace: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
    let pins = &workspace["workspace"]["dependencies"];
    assert!(
        pins.get("windows").is_some(),
        "the windows binding is pinned once at workspace level"
    );
    let mut gated = 0;
    for entry in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        let manifest_path = entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest_path).unwrap();
        let manifest: toml::Value = toml::from_str(&text).unwrap();
        let name = manifest["package"]["name"].as_str().unwrap().to_string();
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = manifest.get(section).and_then(|d| d.as_table()) {
                for dep in ["windows", "windows-core", "windows-sys"] {
                    assert!(
                        !deps.contains_key(dep),
                        "{name}: {dep} must be declared under [target.'cfg(windows)'.dependencies], not [{section}]"
                    );
                }
            }
        }
        if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
            for (cfg, table) in targets {
                let deps = table.get("dependencies").and_then(|d| d.as_table());
                let Some(deps) = deps else { continue };
                for (dep, spec) in deps {
                    if dep.starts_with("windows") {
                        assert_eq!(cfg, "cfg(windows)", "{name}: {dep} under {cfg}");
                        assert_eq!(
                            spec.get("workspace").and_then(|w| w.as_bool()),
                            Some(true),
                            "{name}: {dep} must use the workspace pin, not its own version"
                        );
                        gated += 1;
                    }
                }
            }
        }
    }
    assert!(
        gated >= 3,
        "the Windows-only crates declare the gated binding"
    );
}

fn manual_ids_from_registry() -> BTreeSet<String> {
    tiers()["verification"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["command"]
                .as_str()
                .unwrap()
                .starts_with("cargo xtask manual-record")
        })
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn every_manual_verification_id_has_a_procedure() {
    let root = repo_root();
    let manifest = manual_manifest();
    let procedures: BTreeSet<String> = manifest["procedure"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    let manual = manual_ids_from_registry();
    assert!(!manual.is_empty());
    assert_eq!(
        procedures, manual,
        "every manual-record id has a procedure and every procedure names a registered manual id"
    );
    // Every procedure id is declared by some plan.
    let mut declared = BTreeSet::new();
    for plan in tiers()["plans"].as_array().unwrap() {
        for (id, _) in plan_ids(&root.join(plan.as_str().unwrap())) {
            declared.insert(id);
        }
    }
    for id in &procedures {
        assert!(declared.contains(id), "{id} is not a plan-declared id");
    }
    // Each procedure is complete enough to be performed and digested.
    for p in manifest["procedure"].as_array().unwrap() {
        let id = p["id"].as_str().unwrap();
        assert!(!p["steps"].as_array().unwrap().is_empty(), "{id}: steps");
        assert!(!p["pass_criterion"].as_str().unwrap().is_empty(), "{id}");
        let has_keys = p
            .get("required_observations")
            .and_then(|k| k.as_array())
            .is_some_and(|k| !k.is_empty())
            || p.get("required_observations_from").is_some();
        assert!(
            has_keys,
            "{id}: a record must have declared required observations"
        );
        // Without a record the registered command fails rather than passing vacuously.
        let (code, report, _) = xtask(
            &[
                "manual-record",
                "--id",
                id,
                "--require",
                "pass",
                "--format",
                "json",
            ],
            &root,
        );
        let record = root
            .join(manifest["records_dir"].as_str().unwrap())
            .join(format!("{id}.json"));
        if !record.exists() {
            assert_eq!(code, 1, "{id}");
            assert!(report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f.as_str().unwrap().contains("no record")));
        }
    }
}

/// A manifest in a temporary workspace: one procedure with literal keys, one reading the adapter
/// tables, plus a copy of the real adapter tables.
fn fixture_workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let crates = root.join("crates");
    for entry in std::fs::read_dir(repo_root().join("crates"))
        .unwrap()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("ma-adapter-") && entry.path().join("adapter.toml").is_file() {
            std::fs::create_dir_all(crates.join(&name)).unwrap();
            std::fs::copy(
                entry.path().join("adapter.toml"),
                crates.join(&name).join("adapter.toml"),
            )
            .unwrap();
        }
    }
    std::fs::create_dir_all(root.join("records")).unwrap();
    let manifest = r#"
schema = 1
records_dir = "records"

[[procedure]]
id = "v-fixture-literal"
owner = "tests"
host_profile = "fixture-host"
steps = ["observe a", "observe b"]
pass_criterion = "both observed"
required_observations = ["a", "b"]

[[procedure]]
id = "v-fixture-per-app"
owner = "tests"
host_profile = "fixture-host"
steps = ["observe each application"]
pass_criterion = "one observation per adapter table"
required_observations_from = "adapter-tables"

[[procedure]]
id = "v-fixture-structured"
owner = "tests"
host_profile = "fixture-host"
steps = ["compare both modes"]
pass_criterion = "both modes support the conclusion"
required_observations = ["app"]
observation_required_fields = ["single_process", "process_tree", "requirement"]

[procedure.observation_field_domains]
single_process = ["captured", "silent", "activation-failed"]
process_tree = ["captured", "silent", "activation-failed"]
requirement = ["same", "process-tree-required", "not-activatable"]
"#;
    std::fs::write(root.join("manual-verification.toml"), manifest).unwrap();
    (dir, root)
}

fn digest_of(root: &Path, id: &str) -> String {
    let (_, report, text) = xtask(
        &[
            "manual-record",
            "--id",
            id,
            "--require",
            "pass",
            "--format",
            "json",
        ],
        root,
    );
    report["procedure_digest"]
        .as_str()
        .unwrap_or_else(|| panic!("digest for {id}: {text}"))
        .to_string()
}

fn write_record(root: &Path, id: &str, outcome: &str, observations: &[&str], digest: &str) {
    let obs: serde_json::Map<String, serde_json::Value> = observations
        .iter()
        .map(|k| (k.to_string(), serde_json::Value::String("observed".into())))
        .collect();
    let record = serde_json::json!({
        "id": id,
        "performed_at": "2026-09-04T00:00:00Z",
        "performed_by": "tests",
        "host_profile": "fixture-host",
        "outcome": outcome,
        "observations": obs,
        "procedure_digest": digest,
    });
    std::fs::write(
        root.join("records").join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
}

fn write_record_value(
    root: &Path,
    id: &str,
    outcome: &str,
    observations: serde_json::Value,
    digest: &str,
) {
    let record = serde_json::json!({
        "id": id,
        "performed_at": "2026-09-04T00:00:00Z",
        "performed_by": "tests",
        "host_profile": "fixture-host",
        "outcome": outcome,
        "observations": observations,
        "procedure_digest": digest,
    });
    std::fs::write(
        root.join("records").join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
}

fn manual_record(root: &Path, id: &str) -> (i32, Vec<String>) {
    let (code, report, _) = xtask(
        &[
            "manual-record",
            "--id",
            id,
            "--require",
            "pass",
            "--format",
            "json",
        ],
        root,
    );
    let findings = report["findings"]
        .as_array()
        .map(|a| a.iter().map(|f| f.as_str().unwrap().to_string()).collect())
        .unwrap_or_default();
    (code, findings)
}

#[test]
fn a_record_whose_procedure_changed_is_rejected() {
    let (_dir, root) = fixture_workspace();
    let id = "v-fixture-literal";
    let digest = digest_of(&root, id);
    assert_eq!(digest.len(), 64);
    // Absent record: fails.
    let (code, findings) = manual_record(&root, id);
    assert_eq!(code, 1);
    assert!(findings.iter().any(|f| f.contains("no record")));
    // A complete, passing, current record: passes.
    write_record(&root, id, "pass", &["a", "b"], &digest);
    assert_eq!(manual_record(&root, id).0, 0);
    // Outcome other than pass: fails.
    write_record(&root, id, "fail", &["a", "b"], &digest);
    let (code, findings) = manual_record(&root, id);
    assert_eq!(code, 1);
    assert!(findings.iter().any(|f| f.contains("outcome is fail")));
    // Missing required observation: fails.
    write_record(&root, id, "pass", &["a"], &digest);
    let (code, findings) = manual_record(&root, id);
    assert_eq!(code, 1);
    assert!(findings
        .iter()
        .any(|f| f.contains("omits required observation b")));
    // The procedure text changes: the same record is stale.
    write_record(&root, id, "pass", &["a", "b"], &digest);
    let manifest = std::fs::read_to_string(root.join("manual-verification.toml")).unwrap();
    std::fs::write(
        root.join("manual-verification.toml"),
        manifest.replace("observe b", "observe b twice"),
    )
    .unwrap();
    let (code, findings) = manual_record(&root, id);
    assert_eq!(code, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("performed against procedure digest")),
        "{findings:?}"
    );
    assert_ne!(
        digest_of(&root, id),
        digest,
        "the digest follows the procedure text"
    );
}

#[test]
fn loopback_requirement_record_covers_every_adapter_table() {
    let root = repo_root();
    // The real procedure reads its keys from the adapter tables, not from literals.
    let manifest = manual_manifest();
    let proc_ = manifest["procedure"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"].as_str() == Some("v-win1-loopback-requirement-live-comparison"))
        .expect("procedure declared");
    assert_eq!(
        proc_["required_observations_from"].as_str(),
        Some("adapter-tables")
    );
    assert!(proc_.get("required_observations").is_none());
    let (_, report, text) = xtask(
        &[
            "manual-record",
            "--id",
            "v-win1-loopback-requirement-live-comparison",
            "--require",
            "pass",
            "--format",
            "json",
        ],
        &root,
    );
    let required: BTreeSet<String> = report["required_observations"]
        .as_array()
        .unwrap_or_else(|| panic!("{text}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let mut discovered = BTreeSet::new();
    for entry in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        let table = entry.path().join("adapter.toml");
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("ma-adapter-")
            && table.is_file()
        {
            let v: toml::Value = toml::from_str(&std::fs::read_to_string(table).unwrap()).unwrap();
            discovered.insert(v["id"].as_str().unwrap().to_string());
        }
    }
    assert!(discovered.len() >= 4, "four target applications");
    assert_eq!(
        required, discovered,
        "one required observation per adapter table id"
    );
    // xtask source carries none of those identifiers.
    let src = std::fs::read_to_string(root.join("xtask/src/manual_record.rs")).unwrap();
    for id in &discovered {
        assert!(
            !src.contains(&format!("\"{id}\"")),
            "xtask must not name the service {id}"
        );
    }
    // A record that omits one adapter table is rejected.
    let (_dir, fixture) = fixture_workspace();
    let id = "v-fixture-per-app";
    let digest = digest_of(&fixture, id);
    let all: Vec<&str> = discovered.iter().map(String::as_str).collect();
    write_record(&fixture, id, "pass", &all, &digest);
    assert_eq!(manual_record(&fixture, id).0, 0);
    let missing_one: Vec<&str> = all[1..].to_vec();
    write_record(&fixture, id, "pass", &missing_one, &digest);
    let (code, findings) = manual_record(&fixture, id);
    assert_eq!(code, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.contains(&format!("omits required observation {}", all[0]))),
        "{findings:?}"
    );

    assert_eq!(
        proc_["observation_required_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["single_process", "process_tree", "requirement"]
    );

    // A conclusion without both measured activation results is rejected, as is a value outside
    // the procedure's closed domain.
    let structured_id = "v-fixture-structured";
    let structured_digest = digest_of(&fixture, structured_id);
    write_record_value(
        &fixture,
        structured_id,
        "pass",
        serde_json::json!({"app": {"requirement": "process-tree-required"}}),
        &structured_digest,
    );
    let (code, findings) = manual_record(&fixture, structured_id);
    assert_eq!(code, 1);
    assert!(findings
        .iter()
        .any(|f| f.contains("app omits required field single_process")));
    write_record_value(
        &fixture,
        structured_id,
        "pass",
        serde_json::json!({
            "app": {
                "single_process": "captured",
                "process_tree": "captured",
                "requirement": "guessed"
            }
        }),
        &structured_digest,
    );
    let (code, findings) = manual_record(&fixture, structured_id);
    assert_eq!(code, 1);
    assert!(findings
        .iter()
        .any(|f| f.contains("outside the declared domain")));
    write_record_value(
        &fixture,
        structured_id,
        "pass",
        serde_json::json!({
            "app": {
                "single_process": "silent",
                "process_tree": "captured",
                "requirement": "same"
            }
        }),
        &structured_digest,
    );
    let (code, findings) = manual_record(&fixture, structured_id);
    assert_eq!(code, 1);
    assert!(findings
        .iter()
        .any(|f| f.contains("contradicts its measured activation modes")));
    write_record_value(
        &fixture,
        structured_id,
        "pass",
        serde_json::json!({
            "app": {
                "single_process": "silent",
                "process_tree": "captured",
                "requirement": "process-tree-required"
            }
        }),
        &structured_digest,
    );
    assert_eq!(manual_record(&fixture, structured_id).0, 0);
}
