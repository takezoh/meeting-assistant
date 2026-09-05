//! `cargo xtask manual-record`: an observation the hosted runner cannot make, gated by a check it
//! can (contract-manual-verification-record).
//!
//! `manual-verification.toml` declares one procedure per manual verification id. A performed
//! observation is a committed JSON record. The check fails when the record is absent, when its
//! outcome is not the required one, when it omits a declared required observation, or when the
//! procedure digest it was performed against differs from the current procedure text — so editing
//! a procedure invalidates every record taken against the old one, and a record cannot claim `pass`
//! while leaving part of the procedure's subject unobserved.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "manual-verification.toml";

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    /// Directory (repository-relative) holding `<id>.json` records.
    pub records_dir: String,
    #[serde(default)]
    pub procedure: Vec<Procedure>,
}

/// One manual verification procedure.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Procedure {
    pub id: String,
    pub owner: String,
    pub host_profile: String,
    pub steps: Vec<String>,
    pub pass_criterion: String,
    /// Observation keys the record must carry.
    #[serde(default)]
    pub required_observations: Vec<String>,
    /// Where the record's required observation keys come from instead of a literal list.
    /// `adapter-tables` = one key per `crates/ma-adapter-*/adapter.toml` id, read from the tables
    /// so no service identifier is written into this file or into xtask.
    #[serde(default)]
    pub required_observations_from: Option<String>,
    /// When declared, every required observation's value must be one of these strings.
    #[serde(default)]
    pub observation_domain: Option<Vec<String>>,
    /// Required fields when each observation is a structured per-subject comparison.
    #[serde(default)]
    pub observation_required_fields: Vec<String>,
    /// Optional string domains for fields in a structured observation.
    #[serde(default)]
    pub observation_field_domains: BTreeMap<String, Vec<String>>,
}

/// `null`, an empty string or an empty object is not an observation. Empty arrays are meaningful
/// for procedures whose successful result is "no gaps/conflicts" and are checked semantically.
fn observation_is_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        serde_json::Value::Array(_) => false,
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// A committed observation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Record {
    pub id: String,
    pub performed_at: String,
    pub performed_by: String,
    pub host_profile: String,
    pub outcome: String,
    #[serde(default)]
    pub observations: BTreeMap<String, serde_json::Value>,
    pub procedure_digest: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub ok: bool,
    pub id: String,
    pub record_path: String,
    pub procedure_digest: String,
    pub required_observations: Vec<String>,
    pub findings: Vec<String>,
}

pub fn load_manifest(root: &Path, manifest: Option<&Path>) -> Result<Manifest, String> {
    let path = manifest
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(MANIFEST_FILE));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|e| format!("{} is invalid: {e}", path.display()))?;
    if manifest.schema != 1 {
        return Err(format!(
            "{}: unsupported schema {}",
            path.display(),
            manifest.schema
        ));
    }
    Ok(manifest)
}

/// The adapter table ids under `crates/ma-adapter-*/adapter.toml`, sorted.
pub fn adapter_table_ids(root: &Path) -> Result<Vec<String>, String> {
    let crates = root.join("crates");
    let mut ids = Vec::new();
    let entries =
        std::fs::read_dir(&crates).map_err(|e| format!("cannot read {}: {e}", crates.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("ma-adapter-") {
            continue;
        }
        let table = entry.path().join("adapter.toml");
        if !table.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&table)
            .map_err(|e| format!("cannot read {}: {e}", table.display()))?;
        let value: toml::Value =
            toml::from_str(&text).map_err(|e| format!("{} is invalid: {e}", table.display()))?;
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{} has no id", table.display()))?;
        ids.push(id.to_string());
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err("no adapter table found under crates/ma-adapter-*/adapter.toml".into());
    }
    Ok(ids)
}

/// The observation keys a record for `procedure` must carry.
pub fn required_observations(root: &Path, procedure: &Procedure) -> Result<Vec<String>, String> {
    let mut keys = procedure.required_observations.clone();
    match procedure.required_observations_from.as_deref() {
        None => {}
        Some("adapter-tables") => keys.extend(adapter_table_ids(root)?),
        Some(other) => {
            return Err(format!(
                "{}: unknown required_observations_from source {other}",
                procedure.id
            ))
        }
    }
    keys.sort();
    keys.dedup();
    Ok(keys)
}

/// SHA-256 over the procedure text a record is performed against: host profile, steps, pass
/// criterion and the resolved required observation keys, in a fixed serialisation.
pub fn procedure_digest(procedure: &Procedure, required: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"manual-verification-procedure@1\n");
    hasher.update(b"typed-observation-semantics@3\n");
    hasher.update(procedure.id.as_bytes());
    hasher.update(b"\nhost_profile=");
    hasher.update(procedure.host_profile.as_bytes());
    for (i, step) in procedure.steps.iter().enumerate() {
        hasher.update(format!("\nstep[{i}]=").as_bytes());
        hasher.update(step.as_bytes());
    }
    hasher.update(b"\npass_criterion=");
    hasher.update(procedure.pass_criterion.as_bytes());
    for key in required {
        hasher.update(b"\nobservation=");
        hasher.update(key.as_bytes());
    }
    for value in procedure.observation_domain.iter().flatten() {
        hasher.update(b"\ndomain=");
        hasher.update(value.as_bytes());
    }
    for field in &procedure.observation_required_fields {
        hasher.update(b"\nobservation_field=");
        hasher.update(field.as_bytes());
    }
    for (field, domain) in &procedure.observation_field_domains {
        for value in domain {
            hasher.update(b"\nfield_domain=");
            hasher.update(field.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
        }
    }
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn validate_semantics(
    id: &str,
    observations: &BTreeMap<String, serde_json::Value>,
    target_application_ids: Option<&[String]>,
    findings: &mut Vec<String>,
) {
    let value = |key: &str| observations.get(key);
    let number = |key: &str| value(key).and_then(serde_json::Value::as_f64);
    let truth = |key: &str| value(key).and_then(serde_json::Value::as_bool);
    let concrete_string =
        |item: &serde_json::Value| item.as_str().is_some_and(|text| !text.trim().is_empty());
    let concrete_string_array = |key: &str, min: usize| {
        value(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| items.len() >= min && items.iter().all(concrete_string))
    };
    let require = |ok: bool, message: &str, findings: &mut Vec<String>| {
        if !ok {
            findings.push(format!("{id}: {message}"));
        }
    };

    match id {
        "v-win1-process-identity-live-probe" => {
            let packaged = value("packaged_applications").and_then(serde_json::Value::as_array);
            let unpackaged =
                value("non_packaged_applications").and_then(serde_json::Value::as_array);
            let classified: Vec<&str> = packaged
                .into_iter()
                .chain(unpackaged)
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .collect();
            require(
                packaged.is_some()
                    && unpackaged.is_some()
                    && packaged.unwrap().iter().all(concrete_string)
                    && unpackaged.unwrap().iter().all(concrete_string)
                    && classified.len() >= 4
                    && classified.iter().copied().collect::<BTreeSet<_>>().len()
                        == classified.len(),
                "packaged and non-packaged application arrays must contain distinct, concrete identifiers for all target applications",
                findings,
            );
            require(
                number("package_query_failures") == Some(0.0),
                "package_query_failures must be 0",
                findings,
            );
            require(
                truth("restart_resync_observed") == Some(true),
                "restart_resync_observed must be true",
                findings,
            );
            require(
                concrete_string_array("five_real_fixture_captures", 5)
                    && value("five_real_fixture_captures")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|items| {
                            items.len() == 5
                                && items
                                    .iter()
                                    .filter_map(serde_json::Value::as_str)
                                    .collect::<BTreeSet<_>>()
                                    .len()
                                    == 5
                        }),
                "five_real_fixture_captures must contain five distinct, concrete capture identifiers",
                findings,
            );
            require(
                value("redaction_mapping_recorded")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|map| {
                        !map.is_empty()
                            && map.iter().all(|(source, redacted)| {
                                !source.trim().is_empty() && concrete_string(redacted)
                            })
                    }),
                "redaction_mapping_recorded must map non-empty identifiers to non-empty redacted strings",
                findings,
            );
            require(
                value("five_replays_match_sidecars")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|items| {
                        items.len() == 5 && items.iter().all(|item| item.as_bool() == Some(true))
                    }),
                "five_replays_match_sidecars must contain exactly five true replay results",
                findings,
            );
        }
        "v-win1-mic-use-latency-live" => {
            for key in ["started_latency_ms", "stopped_latency_ms"] {
                require(
                    number(key).is_some_and(|n| (0.0..=1000.0).contains(&n)),
                    &format!("{key} must be between 0 and 1000"),
                    findings,
                );
            }
            require(
                number("consent_corroboration_delay_ms").is_some_and(|n| n >= 0.0),
                "consent_corroboration_delay_ms must be a non-negative number",
                findings,
            );
            require(
                number("inconclusive_consent_only") == Some(0.0),
                "inconclusive_consent_only must be 0",
                findings,
            );
            require(
                number("conflicts") == Some(0.0),
                "conflicts must be 0",
                findings,
            );
        }
        "v-win1-loopback-live-activation" => {
            let expected: Option<BTreeSet<&str>> =
                target_application_ids.map(|ids| ids.iter().map(String::as_str).collect());
            for (key, domain) in [
                ("activation_outcomes", &["Activated", "Fallback"][..]),
                (
                    "capture_modes",
                    &["process_loopback", "system_loopback"][..],
                ),
                ("contamination_risks", &["none", "possible_other_apps"][..]),
            ] {
                require(
                    value(key)
                        .and_then(serde_json::Value::as_object)
                        .is_some_and(|map| {
                            let keys: BTreeSet<&str> = map.keys().map(String::as_str).collect();
                            let covers_targets = expected
                                .as_ref()
                                .map_or_else(|| keys.len() >= 4, |wanted| keys == *wanted);
                            covers_targets
                                && map.values().all(|item| {
                                    item.as_str().is_some_and(|text| domain.contains(&text))
                                })
                        }),
                    &format!(
                        "{key} must contain every target application with a value in {domain:?}"
                    ),
                    findings,
                );
            }
        }
        "v-win1-mic-endpoint-live" => {
            require(
                truth("initial_endpoint_matches_session") == Some(true),
                "initial_endpoint_matches_session must be true",
                findings,
            );
            require(
                truth("successor_track_on_switch") == Some(true),
                "successor_track_on_switch must be true",
                findings,
            );
            require(
                concrete_string_array("selection_history", 2)
                    && value("selection_history")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|items| {
                            items
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<BTreeSet<_>>()
                                .len()
                                >= 2
                        }),
                "selection_history must contain distinct, concrete initial and successor endpoints",
                findings,
            );
        }
        "v-win1-leak-live-per-app" => {
            for (key, observation) in observations {
                let Some(object) = observation.as_object() else {
                    findings.push(format!("{id}: observation {key} must be an object"));
                    continue;
                };
                let outcome = object.get("outcome").and_then(serde_json::Value::as_str);
                require(
                    matches!(
                        outcome,
                        Some("measured" | "no_qualifying_window" | "inconclusive_alignment")
                    ),
                    &format!("observation {key}.outcome is invalid"),
                    findings,
                );
                require(
                    object
                        .get("alignment_uncertainty_ms")
                        .and_then(serde_json::Value::as_u64)
                        .is_some(),
                    &format!("observation {key} must record alignment_uncertainty_ms"),
                    findings,
                );
                if outcome == Some("measured") {
                    for field in ["erl_db", "loopback_rms_dbfs", "microphone_rms_dbfs"] {
                        require(
                            object
                                .get(field)
                                .and_then(serde_json::Value::as_f64)
                                .is_some(),
                            &format!("observation {key}.{field} must be numeric when measured"),
                            findings,
                        );
                    }
                }
            }
        }
        "v-win1-two-hour-live" => {
            let wall = number("wall_clock_duration_s");
            let captured = number("captured_sample_duration_s");
            require(
                wall.is_some_and(|n| n >= 7200.0),
                "wall_clock_duration_s must be at least 7200",
                findings,
            );
            require(
                value("manifest_vs_directory").is_some_and(|item| {
                    item == &serde_json::Value::Bool(true) || item.as_str() == Some("match")
                }),
                "manifest_vs_directory must be true or 'match'",
                findings,
            );
            require(
                value("gap_records")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(Vec::is_empty),
                "gap_records must be an empty array",
                findings,
            );
            require(
                matches!((wall, captured), (Some(w), Some(c)) if w > 0.0 && ((c - w).abs() / w) <= 0.01),
                "captured_sample_duration_s must be within one percent of wall clock",
                findings,
            );
        }
        "v-win1-loopback-requirement-live-comparison" | "v-fixture-structured" => {
            for (key, observation) in observations {
                let Some(object) = observation.as_object() else {
                    continue;
                };
                let tuple = (
                    object
                        .get("single_process")
                        .and_then(serde_json::Value::as_str),
                    object
                        .get("process_tree")
                        .and_then(serde_json::Value::as_str),
                    object
                        .get("requirement")
                        .and_then(serde_json::Value::as_str),
                );
                let valid = matches!(
                    tuple,
                    (Some("captured"), Some("captured"), Some("same"))
                        | (
                            Some("silent" | "activation-failed"),
                            Some("captured"),
                            Some("process-tree-required")
                        )
                        | (
                            _,
                            Some("silent" | "activation-failed"),
                            Some("not-activatable")
                        )
                );
                require(
                    valid,
                    &format!(
                        "observation {key} requirement contradicts its measured activation modes"
                    ),
                    findings,
                );
            }
        }
        "v-win1-extension-live-chrome" => {
            for key in [
                "meeting_tab_204",
                "fields_only_declared",
                "stale_token_stops_posting",
            ] {
                require(
                    truth(key) == Some(true),
                    &format!("{key} must be true"),
                    findings,
                );
            }
        }
        "v-win1-browser-loopback-policy-observed" => {
            for key in ["chrome_reaches_listener", "edge_reaches_listener"] {
                require(
                    value(key)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|s| matches!(s, "reached" | "blocked")),
                    &format!("{key} must be reached or blocked"),
                    findings,
                );
            }
            require(
                truth("endpoint_json_same_user_readable").is_some(),
                "endpoint_json_same_user_readable must be a boolean observation",
                findings,
            );
        }
        _ => {}
    }
}

pub fn record_path(root: &Path, manifest: &Manifest, id: &str) -> PathBuf {
    root.join(&manifest.records_dir).join(format!("{id}.json"))
}

/// Checks the record for `id` against its procedure. `require` is the outcome the record must
/// state (normally `pass`).
pub fn check(
    root: &Path,
    manifest_path: Option<&Path>,
    id: &str,
    require: &str,
) -> Result<Report, String> {
    let manifest = load_manifest(root, manifest_path)?;
    let procedure = manifest
        .procedure
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("{id}: no procedure declared in {MANIFEST_FILE}"))?;
    let required = required_observations(root, procedure)?;
    let digest = procedure_digest(procedure, &required);
    let path = record_path(root, &manifest, id);
    let mut findings = Vec::new();
    match std::fs::read(&path) {
        Err(_) => findings.push(format!(
            "{id}: no record at {} — the observation has not been performed and committed",
            path.display()
        )),
        Ok(bytes) => match serde_json::from_slice::<Record>(&bytes) {
            Err(e) => findings.push(format!("{id}: record is not a valid record: {e}")),
            Ok(record) => {
                if record.id != id {
                    findings.push(format!(
                        "{id}: record names {} instead of this id",
                        record.id
                    ));
                }
                if record.outcome != require {
                    findings.push(format!(
                        "{id}: record outcome is {} but {require} is required",
                        record.outcome
                    ));
                }
                if record.host_profile != procedure.host_profile {
                    findings.push(format!(
                        "{id}: record host profile {} differs from the procedure's {}",
                        record.host_profile, procedure.host_profile
                    ));
                }
                for key in &required {
                    match record.observations.get(key) {
                        None => {
                            findings.push(format!("{id}: record omits required observation {key}"))
                        }
                        Some(value) if observation_is_empty(value) => findings.push(format!(
                            "{id}: required observation {key} carries no value (null or empty)"
                        )),
                        Some(value) => {
                            if let Some(domain) = &procedure.observation_domain {
                                let ok = value
                                    .as_str()
                                    .is_some_and(|s| domain.iter().any(|d| d == s));
                                if !ok {
                                    findings.push(format!(
                                        "{id}: observation {key} = {value} is outside the declared domain {domain:?}"
                                    ));
                                }
                            }
                            if !procedure.observation_required_fields.is_empty() {
                                let Some(object) = value.as_object() else {
                                    findings.push(format!(
                                        "{id}: observation {key} must be an object with fields {:?}",
                                        procedure.observation_required_fields
                                    ));
                                    continue;
                                };
                                for field in &procedure.observation_required_fields {
                                    match object.get(field) {
                                        None => findings.push(format!(
                                            "{id}: observation {key} omits required field {field}"
                                        )),
                                        Some(field_value) if observation_is_empty(field_value) => {
                                            findings.push(format!(
                                                "{id}: observation {key}.{field} carries no value (null or empty)"
                                            ))
                                        }
                                        Some(field_value) => {
                                            if let Some(domain) =
                                                procedure.observation_field_domains.get(field)
                                            {
                                                let ok = field_value.as_str().is_some_and(|s| {
                                                    domain.iter().any(|allowed| allowed == s)
                                                });
                                                if !ok {
                                                    findings.push(format!(
                                                        "{id}: observation {key}.{field} = {field_value} is outside the declared domain {domain:?}"
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let target_application_ids = if id == "v-win1-loopback-live-activation" {
                    Some(adapter_table_ids(root)?)
                } else {
                    None
                };
                validate_semantics(
                    id,
                    &record.observations,
                    target_application_ids.as_deref(),
                    &mut findings,
                );
                if record.procedure_digest != digest {
                    findings.push(format!(
                        "{id}: record was performed against procedure digest {} but the current procedure is {digest}; redo the observation",
                        record.procedure_digest
                    ));
                }
            }
        },
    }
    Ok(Report {
        ok: findings.is_empty(),
        id: id.to_string(),
        record_path: path.display().to_string(),
        procedure_digest: digest,
        required_observations: required,
        findings,
    })
}

pub fn print_text(report: &Report) {
    println!(
        "manual-record {}: record {} (procedure digest {})",
        report.id, report.record_path, report.procedure_digest
    );
    for f in &report.findings {
        println!("FINDING {f}");
    }
    println!("{}", if report.ok { "OK" } else { "FAILED" });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(value: serde_json::Value) -> BTreeMap<String, serde_json::Value> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn typed_semantics_reject_false_and_out_of_range_claims() {
        let mut findings = Vec::new();
        validate_semantics(
            "v-win1-extension-live-chrome",
            &map(serde_json::json!({
                "meeting_tab_204": false,
                "fields_only_declared": true,
                "stale_token_stops_posting": true
            })),
            None,
            &mut findings,
        );
        assert!(findings.iter().any(|item| item.contains("meeting_tab_204")));

        findings.clear();
        validate_semantics(
            "v-win1-mic-use-latency-live",
            &map(serde_json::json!({
                "started_latency_ms": 1001,
                "stopped_latency_ms": 1000,
                "consent_corroboration_delay_ms": 0,
                "inconclusive_consent_only": 0,
                "conflicts": 0
            })),
            None,
            &mut findings,
        );
        assert!(findings
            .iter()
            .any(|item| item.contains("started_latency_ms")));
    }

    #[test]
    fn process_identity_semantics_reject_placeholder_evidence() {
        let mut findings = Vec::new();
        validate_semantics(
            "v-win1-process-identity-live-probe",
            &map(serde_json::json!({
                "packaged_applications": [null, null],
                "non_packaged_applications": ["", "app-d"],
                "package_query_failures": 0,
                "restart_resync_observed": true,
                "five_real_fixture_captures": [null, null, null, null, null],
                "redaction_mapping_recorded": {"host-a": null},
                "five_replays_match_sidecars": [false, false, false, false, false]
            })),
            None,
            &mut findings,
        );
        for field in [
            "packaged and non-packaged",
            "five_real_fixture_captures",
            "redaction_mapping_recorded",
            "five_replays_match_sidecars",
        ] {
            assert!(
                findings.iter().any(|item| item.contains(field)),
                "missing finding for {field}: {findings:?}"
            );
        }
    }

    #[test]
    fn loopback_semantics_reject_placeholder_map_values() {
        let mut findings = Vec::new();
        let target_ids = ["a", "b", "c", "d"].map(str::to_string);
        validate_semantics(
            "v-win1-loopback-live-activation",
            &map(serde_json::json!({
                "activation_outcomes": {"a": null, "b": null, "c": null, "d": null},
                "capture_modes": {"a": null, "b": null, "c": null, "d": null},
                "contamination_risks": {"a": null, "b": null, "c": null, "d": null}
            })),
            Some(&target_ids),
            &mut findings,
        );
        for field in [
            "activation_outcomes",
            "capture_modes",
            "contamination_risks",
        ] {
            assert!(
                findings.iter().any(|item| item.contains(field)),
                "missing finding for {field}: {findings:?}"
            );
        }

        findings.clear();
        validate_semantics(
            "v-win1-loopback-live-activation",
            &map(serde_json::json!({
                "activation_outcomes": {"a": "Activated", "b": "Fallback", "c": "Activated", "wrong": "Activated"},
                "capture_modes": {"a": "process_loopback", "b": "system_loopback", "c": "process_loopback", "wrong": "process_loopback"},
                "contamination_risks": {"a": "none", "b": "possible_other_apps", "c": "none", "wrong": "none"}
            })),
            Some(&target_ids),
            &mut findings,
        );
        assert!(
            findings.len() == 3,
            "each map with a non-target key should be rejected: {findings:?}"
        );
    }

    #[test]
    fn concrete_manual_evidence_examples_are_accepted() {
        let mut findings = Vec::new();
        validate_semantics(
            "v-win1-process-identity-live-probe",
            &map(serde_json::json!({
                "packaged_applications": ["app-a", "app-b"],
                "non_packaged_applications": ["app-c", "app-d"],
                "package_query_failures": 0,
                "restart_resync_observed": true,
                "five_real_fixture_captures": ["capture-1", "capture-2", "capture-3", "capture-4", "capture-5"],
                "redaction_mapping_recorded": {"host-a": "host-1"},
                "five_replays_match_sidecars": [true, true, true, true, true]
            })),
            None,
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");

        findings.clear();
        let target_ids = ["a", "b", "c", "d"].map(str::to_string);
        validate_semantics(
            "v-win1-loopback-live-activation",
            &map(serde_json::json!({
                "activation_outcomes": {"a": "Activated", "b": "Fallback", "c": "Activated", "d": "Activated"},
                "capture_modes": {"a": "process_loopback", "b": "system_loopback", "c": "process_loopback", "d": "process_loopback"},
                "contamination_risks": {"a": "none", "b": "possible_other_apps", "c": "none", "d": "none"}
            })),
            Some(&target_ids),
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");

        findings.clear();
        validate_semantics(
            "v-win1-mic-endpoint-live",
            &map(serde_json::json!({
                "initial_endpoint_matches_session": true,
                "successor_track_on_switch": true,
                "selection_history": ["endpoint-a", "endpoint-b"]
            })),
            None,
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn endpoint_semantics_reject_non_concrete_or_repeated_history() {
        for history in [
            serde_json::json!([null, null]),
            serde_json::json!(["", "endpoint-b"]),
            serde_json::json!(["endpoint-a", "endpoint-a"]),
        ] {
            let mut findings = Vec::new();
            validate_semantics(
                "v-win1-mic-endpoint-live",
                &map(serde_json::json!({
                    "initial_endpoint_matches_session": true,
                    "successor_track_on_switch": true,
                    "selection_history": history
                })),
                None,
                &mut findings,
            );
            assert!(
                findings
                    .iter()
                    .any(|item| item.contains("selection_history")),
                "history should be rejected: {findings:?}"
            );
        }
    }

    #[test]
    fn two_hour_semantics_accept_empty_gap_array_but_reject_duration_mismatch() {
        let mut findings = Vec::new();
        validate_semantics(
            "v-win1-two-hour-live",
            &map(serde_json::json!({
                "target_application": "example",
                "wall_clock_duration_s": 7200,
                "captured_sample_duration_s": 7200,
                "manifest_vs_directory": true,
                "gap_records": []
            })),
            None,
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");
        findings.clear();
        validate_semantics(
            "v-win1-two-hour-live",
            &map(serde_json::json!({
                "target_application": "example",
                "wall_clock_duration_s": 7200,
                "captured_sample_duration_s": 7000,
                "manifest_vs_directory": true,
                "gap_records": []
            })),
            None,
            &mut findings,
        );
        assert!(findings
            .iter()
            .any(|item| item.contains("within one percent")));
    }
}
