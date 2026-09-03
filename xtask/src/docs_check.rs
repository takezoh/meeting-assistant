//! `cargo xtask docs-check`: the repository-owned, CI-runnable form of contract-docs-conformance.
//! Five rules, each selectable with `--rule`: `adr-placement`, `design-set`, `change-members`,
//! `promotion-none` and `schema` (frontmatter validated against docs/schemas, the same schemas the
//! dev-docs tooling uses), so contract-docs-conformance runs in a fresh clone and in CI.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const RULES: [&str; 5] = [
    "adr-placement",
    "design-set",
    "change-members",
    "promotion-none",
    "schema",
];
pub const DESIGN_DOCS: [&str; 5] = [
    "module-boundaries",
    "session-lifecycle",
    "recording-artifact-model",
    "threat-model",
    "credential-policy",
];
const CHANGE_ID: &str = "change-20260903-phase0-repository-and-contracts";

#[derive(Debug, Serialize)]
pub struct Finding {
    pub rule: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub rules_checked: Vec<String>,
    pub findings: Vec<Finding>,
}

fn frontmatter(path: &Path) -> Result<serde_yaml::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| format!("{}: no frontmatter", path.display()))?;
    let end = rest
        .find("\n---\n")
        .ok_or_else(|| format!("{}: unterminated frontmatter", path.display()))?;
    serde_yaml::from_str(&rest[..end]).map_err(|e| format!("{}: {e}", path.display()))
}

fn list(value: &serde_yaml::Value, key: &str) -> Vec<serde_yaml::Value> {
    value
        .get(key)
        .and_then(|v| v.as_sequence())
        .cloned()
        .unwrap_or_default()
}

fn check_adr_placement(root: &Path, findings: &mut Vec<Finding>) {
    let dir = root.join("docs/adr");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.sort();
    if paths.is_empty() {
        findings.push(Finding {
            rule: "adr-placement".into(),
            path: dir.display().to_string(),
            detail: "no ADRs".into(),
        });
    }
    for path in paths {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let placed = name.len() > 18
            && name.starts_with("adr-")
            && name.ends_with(".md")
            && name[4..12].chars().all(|c| c.is_ascii_digit())
            && &name[12..13] == "-";
        if !placed {
            findings.push(Finding {
                rule: "adr-placement".into(),
                path: rel.clone(),
                detail: "not at docs/adr/adr-YYYYMMDD-slug.md".into(),
            });
            continue;
        }
        let fm = match frontmatter(&path) {
            Ok(fm) => fm,
            Err(e) => {
                findings.push(Finding {
                    rule: "adr-placement".into(),
                    path: rel,
                    detail: e,
                });
                continue;
            }
        };
        if list(&fm, "decision_makers").is_empty() {
            findings.push(Finding {
                rule: "adr-placement".into(),
                path: rel.clone(),
                detail: "decision_makers is empty".into(),
            });
        }
        let consequences = fm
            .get("consequences")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);
        for pole in ["positive", "negative", "neutral"] {
            if list(&consequences, pole).is_empty() {
                findings.push(Finding {
                    rule: "adr-placement".into(),
                    path: rel.clone(),
                    detail: format!("consequences.{pole} is empty"),
                });
            }
        }
        let status = fm.get("status").and_then(|s| s.as_str()).unwrap_or("");
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let started_proposed = status == "proposed" || body.contains("from=\"proposed\"");
        if !started_proposed {
            findings.push(Finding {
                rule: "adr-placement".into(),
                path: rel.clone(),
                detail: format!("status {status:?} without a recorded transition from proposed"),
            });
        }
    }
}

/// Every `id = "..."` registered in verification-tiers.toml.
fn registered_verification_ids(root: &Path) -> std::collections::BTreeSet<String> {
    let text = std::fs::read_to_string(root.join("verification-tiers.toml")).unwrap_or_default();
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("id = \""))
        .map(|rest| rest.trim_end_matches('"').to_string())
        .collect()
}

/// The `v-...` identifiers a statement cites.
fn cited_ids(statement: &str) -> Vec<String> {
    statement
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| w.starts_with("v-") && w.len() > 2)
        .map(|w| w.trim_end_matches('-').to_string())
        .collect()
}

fn check_design_set(root: &Path, findings: &mut Vec<Finding>) {
    let registered = registered_verification_ids(root);
    let dir = root.join("docs/design");
    let mut present: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .trim_end_matches(".md")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();
    present.sort();
    let mut expected: Vec<String> = DESIGN_DOCS.iter().map(|d| d.to_string()).collect();
    expected.sort();
    if present != expected {
        findings.push(Finding {
            rule: "design-set".into(),
            path: "docs/design".into(),
            detail: format!("expected exactly {expected:?}, found {present:?}"),
        });
    }
    let required = [
        "id",
        "kind",
        "title",
        "status",
        "scope_type",
        "responsibilities",
        "invariants",
        "boundaries",
        "variability",
        "capabilities",
        "failure_responsibilities",
        "trust_boundaries",
        "compatibility_policies",
    ];
    for doc in DESIGN_DOCS {
        let path = dir.join(format!("{doc}.md"));
        let rel = format!("docs/design/{doc}.md");
        let fm = match frontmatter(&path) {
            Ok(fm) => fm,
            Err(e) => {
                findings.push(Finding {
                    rule: "design-set".into(),
                    path: rel,
                    detail: e,
                });
                continue;
            }
        };
        for key in required {
            if fm.get(key).is_none() {
                findings.push(Finding {
                    rule: "design-set".into(),
                    path: rel.clone(),
                    detail: format!("missing {key}"),
                });
            }
        }
        if fm.get("id").and_then(|v| v.as_str()) != Some(&format!("design-{doc}")) {
            findings.push(Finding {
                rule: "design-set".into(),
                path: rel.clone(),
                detail: format!("id must be design-{doc}"),
            });
        }
        let invariants = list(&fm, "invariants");
        if invariants.is_empty() {
            findings.push(Finding {
                rule: "design-set".into(),
                path: rel.clone(),
                detail: "no invariants".into(),
            });
        }
        for inv in invariants {
            let enforcement = inv
                .get("enforcement")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let statement = inv.get("statement").and_then(|v| v.as_str()).unwrap_or("");
            let cited = cited_ids(statement);
            if !["test", "contract", "conformance", "review"].contains(&enforcement) {
                findings.push(Finding {
                    rule: "design-set".into(),
                    path: rel.clone(),
                    detail: format!("invariant {:?} has no enforcement", inv.get("id")),
                });
            } else if enforcement == "test" && cited.is_empty() {
                findings.push(Finding {
                    rule: "design-set".into(),
                    path: rel.clone(),
                    detail: format!(
                        "invariant {:?} is enforced by test but cites no registered v-* check id",
                        inv.get("id")
                    ),
                });
            }
            // a cited check must exist: an invariant nobody checks is a wish
            for id in cited {
                if !registered.contains(&id) {
                    findings.push(Finding { rule: "design-set".into(), path: rel.clone(), detail: format!("invariant {:?} cites {id}, which is not registered in verification-tiers.toml", inv.get("id")) });
                }
            }
        }
    }
}

fn check_change_members(root: &Path, findings: &mut Vec<Finding>) {
    let change = root.join("docs/changes").join(CHANGE_ID).join("change.md");
    let fm = match frontmatter(&change) {
        Ok(fm) => fm,
        Err(e) => {
            findings.push(Finding {
                rule: "change-members".into(),
                path: change.display().to_string(),
                detail: e,
            });
            return;
        }
    };
    let members = list(&fm, "members");
    let mut roles: Vec<String> = members
        .iter()
        .filter_map(|m| m.get("role").and_then(|r| r.as_str()).map(str::to_string))
        .collect();
    roles.sort();
    for role in ["implementation", "requirements", "verification"] {
        if !roles.iter().any(|r| r == role) {
            findings.push(Finding {
                rule: "change-members".into(),
                path: format!("docs/changes/{CHANGE_ID}/change.md"),
                detail: format!("member role {role} missing"),
            });
        }
    }
    for member in members {
        let rel = member.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let path = root.join("docs").join(rel);
        let body_len = std::fs::read_to_string(&path)
            .map(|t| {
                t.split("\n---\n")
                    .nth(1)
                    .map(|b| b.trim().len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        if body_len < 200 {
            findings.push(Finding {
                rule: "change-members".into(),
                path: format!("docs/{rel}"),
                detail: format!("member body is empty or missing ({body_len} bytes)"),
            });
        }
    }
}

fn check_promotion_none(root: &Path, findings: &mut Vec<Finding>) {
    let rel = format!("docs/changes/{CHANGE_ID}/change.md");
    let fm = match frontmatter(&root.join(&rel)) {
        Ok(fm) => fm,
        Err(e) => {
            findings.push(Finding {
                rule: "promotion-none".into(),
                path: rel,
                detail: e,
            });
            return;
        }
    };
    let entries = list(&fm, "promotion");
    if entries.is_empty() {
        findings.push(Finding {
            rule: "promotion-none".into(),
            path: rel.clone(),
            detail: "promotion manifest is absent".into(),
        });
    }
    for entry in entries {
        let action = entry.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let target = entry
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let reason = entry.get("reason").and_then(|v| v.as_str()).unwrap_or("");
        if action != "none" || target != "none" {
            findings.push(Finding { rule: "promotion-none".into(), path: rel.clone(), detail: format!("promotion entry names action {action:?} target {target:?}; this change creates the first design documents and promotes nothing") });
        }
        if reason.trim().is_empty() {
            findings.push(Finding {
                rule: "promotion-none".into(),
                path: rel.clone(),
                detail: "promotion none without a reason".into(),
            });
        }
    }
}

fn kind_of(fm: &serde_yaml::Value, rel: &str) -> Option<String> {
    if let Some(kind) = fm.get("kind").and_then(|k| k.as_str()) {
        return Some(kind.to_string());
    }
    // change members carry `change:` and `role:` instead of a kind
    if fm.get("role").is_some() && rel.contains("/changes/") {
        return Some("change-member".to_string());
    }
    None
}

/// Validate every document's frontmatter under docs/ against docs/schemas/<kind>.schema.json,
/// the same schemas the dev-docs tooling uses (v-docs-schema-conformance in a fresh clone).
fn check_schema(root: &Path, findings: &mut Vec<Finding>) {
    let schemas_dir = root.join("docs/schemas");
    let mut options = jsonschema::options();
    let mut schemas: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    let entries = match std::fs::read_dir(&schemas_dir) {
        Ok(e) => e,
        Err(e) => {
            findings.push(Finding {
                rule: "schema".into(),
                path: "docs/schemas".into(),
                detail: e.to_string(),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|x| x == "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                findings.push(Finding {
                    rule: "schema".into(),
                    path: path.display().to_string(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        if let Some(id) = value.get("$id").and_then(|v| v.as_str()) {
            if let Ok(resource) = jsonschema::Resource::from_contents(value.clone()) {
                options.with_resource(id.to_string(), resource);
            }
        }
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches(".schema.json")
            .trim_end_matches(".json")
            .to_string();
        schemas.insert(name, value);
    }
    let mut docs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.join("docs")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            if p.is_dir() {
                if !rel.contains("docs/schemas")
                    && !rel.contains("docs/index")
                    && !rel.contains("/design-plan")
                {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "md") {
                docs.push(p);
            }
        }
    }
    docs.sort();
    let mut validated = 0;
    for doc in docs {
        let rel = doc.strip_prefix(root).unwrap_or(&doc).display().to_string();
        let Ok(fm) = frontmatter(&doc) else { continue };
        let Some(kind) = kind_of(&fm, &rel) else {
            continue;
        };
        let Some(schema) = schemas.get(&kind) else {
            findings.push(Finding {
                rule: "schema".into(),
                path: rel,
                detail: format!("no schema for kind {kind}"),
            });
            continue;
        };
        let validator = match options.clone().build(schema) {
            Ok(v) => v,
            Err(e) => {
                findings.push(Finding {
                    rule: "schema".into(),
                    path: rel,
                    detail: format!("schema {kind} does not compile: {e}"),
                });
                continue;
            }
        };
        let instance: serde_json::Value = match serde_json::to_value(&fm) {
            Ok(v) => v,
            Err(e) => {
                findings.push(Finding {
                    rule: "schema".into(),
                    path: rel,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        for e in validator.iter_errors(&instance) {
            findings.push(Finding {
                rule: "schema".into(),
                path: rel.clone(),
                detail: format!("{} at {}", e, e.instance_path),
            });
        }
        validated += 1;
    }
    if validated == 0 {
        findings.push(Finding {
            rule: "schema".into(),
            path: "docs".into(),
            detail: "no document was validated".into(),
        });
    }
}

pub fn run(root: &Path, rule: Option<&str>) -> Result<Report, String> {
    let selected: Vec<&str> = match rule {
        Some(r) if RULES.contains(&r) => vec![r],
        Some(r) => return Err(format!("unknown rule {r}; expected one of {RULES:?}")),
        None => RULES.to_vec(),
    };
    let mut findings = Vec::new();
    for r in &selected {
        match *r {
            "adr-placement" => check_adr_placement(root, &mut findings),
            "design-set" => check_design_set(root, &mut findings),
            "change-members" => check_change_members(root, &mut findings),
            "promotion-none" => check_promotion_none(root, &mut findings),
            "schema" => check_schema(root, &mut findings),
            _ => unreachable!(),
        }
    }
    Ok(Report {
        rules_checked: selected.iter().map(|s| s.to_string()).collect(),
        findings,
    })
}

pub fn print_text(report: &Report) {
    for f in &report.findings {
        println!("FINDING [{}] {}: {}", f.rule, f.path, f.detail);
    }
    if report.findings.is_empty() {
        println!(
            "OK: docs conformance ({} rule(s))",
            report.rules_checked.len()
        );
    } else {
        println!("{} finding(s)", report.findings.len());
    }
}
