//! Verification tier registry checks and tier runner (contract-verification-tiering).

use cargo_metadata::MetadataCommand;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct TierFile {
    /// The single-plan form; still accepted and unioned with `plans`.
    #[serde(default)]
    plan: Option<String>,
    /// Every canonical plan whose declared verification ids form the registered set
    /// (contract-windows-tier-verification-registration).
    #[serde(default)]
    plans: Vec<String>,
    #[serde(default)]
    tiers: BTreeMap<String, TierDecl>,
    #[serde(default)]
    verification: Vec<Registration>,
}

impl TierFile {
    /// The plan paths in declaration order, the single field first, duplicates removed.
    fn plan_paths(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in self.plan.iter().chain(self.plans.iter()) {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
        out
    }
}

#[derive(Debug, Deserialize, Default)]
struct TierDecl {
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    core_crates: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Registration {
    pub id: String,
    pub tier: String,
    #[serde(default)]
    pub plan_tier: Option<String>,
    #[serde(default)]
    pub contract: Option<String>,
    pub command: String,
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub ok: bool,
    pub mode: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub runs: Vec<Run>,
}

#[derive(Debug, Serialize)]
pub struct Run {
    pub id: String,
    pub command: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

fn load_tiers(root: &Path, tiers: Option<&Path>) -> Result<(TierFile, PathBuf), String> {
    let path = tiers
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("verification-tiers.toml"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let file: TierFile =
        toml::from_str(&text).map_err(|e| format!("{} is invalid: {e}", path.display()))?;
    Ok((file, path))
}

/// Verification ids declared by the canonical plan, with their plan tier (T0/T1/T2).
fn plan_verifications(plan_path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(plan_path)
        .map_err(|e| format!("cannot read plan {}: {e}", plan_path.display()))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| format!("plan {} is not valid YAML: {e}", plan_path.display()))?;
    let mut out = BTreeMap::new();
    let contracts = doc
        .get("contracts")
        .and_then(|c| c.as_sequence())
        .ok_or("plan has no contracts sequence")?;
    for contract in contracts {
        for v in contract
            .get("verification")
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
        {
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .ok_or("verification without id")?;
            let tier = v.get("tier").and_then(|t| t.as_str()).unwrap_or("T0");
            out.insert(id.to_string(), tier.to_string());
        }
    }
    if out.is_empty() {
        return Err("plan declares no verification ids".into());
    }
    Ok(out)
}

pub fn check_registration(
    root: &Path,
    plan: Option<&Path>,
    tiers: Option<&Path>,
) -> Result<Report, String> {
    let (file, tiers_path) = load_tiers(root, tiers)?;
    let plan_paths: Vec<PathBuf> = match plan {
        Some(p) => vec![p.to_path_buf()],
        None => {
            let paths = file.plan_paths();
            if paths.is_empty() {
                return Err(
                    "verification-tiers.toml declares no plan path (plan or plans) and --plan was not given"
                        .into(),
                );
            }
            paths.iter().map(|p| root.join(p)).collect()
        }
    };
    // The registered set is the union of every declared plan; an id declared by two plans is a
    // plan defect, not something the registry can resolve.
    let mut declared: BTreeMap<String, String> = BTreeMap::new();
    let mut findings = Vec::new();
    for path in &plan_paths {
        for (id, tier) in plan_verifications(path)? {
            if let Some(previous) = declared.insert(id.clone(), tier) {
                findings.push(format!(
                    "{id}: declared by more than one plan (previously as {previous}); every id belongs to exactly one plan"
                ));
            }
        }
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for reg in &file.verification {
        *seen.entry(reg.id.clone()).or_default() += 1;
        let Some(tier_decl) = file.tiers.get(&reg.tier) else {
            findings.push(format!(
                "{}: registered in undeclared tier {}",
                reg.id, reg.tier
            ));
            continue;
        };
        match declared.get(&reg.id) {
            None => findings.push(format!(
                "{}: registered but not declared by the plan (stale registration)",
                reg.id
            )),
            Some(plan_tier) => {
                let expected = if plan_tier == "T2" {
                    "windows"
                } else {
                    "portable"
                };
                if reg.tier != expected {
                    findings.push(format!(
                        "{}: plan tier {plan_tier} must be registered in tier {expected}, found {}",
                        reg.id, reg.tier
                    ));
                }
            }
        }
        let tier_is_platform_bound = tier_decl.platform.is_some();
        match (&reg.platform, tier_is_platform_bound) {
            (None, true) => findings.push(format!("{}: in platform-bound tier {} but nothing marks it platform-bound (add platform = \"{}\")", reg.id, reg.tier, tier_decl.platform.clone().unwrap_or_default())),
            (Some(p), false) => findings.push(format!("{}: marked platform = \"{p}\" but registered in the portable tier {}", reg.id, reg.tier)),
            (Some(p), true) if Some(p) != tier_decl.platform.as_ref() => findings.push(format!("{}: platform {p} does not match tier {}", reg.id, reg.tier)),
            _ => {}
        }
        if reg.command.trim().is_empty() {
            findings.push(format!("{}: registration has no command", reg.id));
        }
    }
    for (id, count) in &seen {
        if *count > 1 {
            findings.push(format!(
                "{id}: registered {count} times; every id must appear exactly once"
            ));
        }
    }
    for (id, tier) in &declared {
        if !seen.contains_key(id) {
            findings.push(format!(
                "{id}: declared by the plan as {tier} but absent from {}",
                tiers_path.display()
            ));
        }
    }
    let windows = file
        .verification
        .iter()
        .filter(|r| r.tier == "windows")
        .count();
    let ok = findings.is_empty();
    Ok(Report {
        ok,
        mode: "check-registration".into(),
        summary: format!(
            "{} plan verification ids, {} registrations ({windows} windows), {} finding(s)",
            declared.len(),
            file.verification.len(),
            findings.len()
        ),
        findings,
        runs: Vec::new(),
    })
}

fn workspace_crates(root: &Path) -> Result<BTreeSet<String>, String> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .map_err(|e| format!("cargo metadata failed: {e}"))?;
    let members: BTreeSet<_> = metadata.workspace_members.iter().cloned().collect();
    Ok(metadata
        .packages
        .iter()
        .filter(|p| members.contains(&p.id))
        .map(|p| p.name.to_string())
        .collect())
}

fn target_crate(command: &str) -> Option<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    parts
        .iter()
        .position(|p| *p == "-p" || *p == "--package")
        .and_then(|i| parts.get(i + 1))
        .map(|s| s.to_string())
}

pub fn run_tier(
    root: &Path,
    tier: &str,
    tiers: Option<&Path>,
    strict: bool,
) -> Result<Report, String> {
    let (file, _) = load_tiers(root, tiers)?;
    let decl = file
        .tiers
        .get(tier)
        .ok_or_else(|| format!("tier {tier} is not declared in verification-tiers.toml"))?;
    if let Some(platform) = &decl.platform {
        let host_ok = match platform.as_str() {
            "windows" => cfg!(windows),
            "linux" => cfg!(target_os = "linux"),
            "macos" => cfg!(target_os = "macos"),
            other => return Err(format!("unknown platform {other}")),
        };
        if !host_ok {
            return Ok(Report {
                ok: false,
                mode: format!("tier:{tier}"),
                summary: format!("tier {tier} requires a {platform} host; this host is {}. The tier is failed, never skipped.", std::env::consts::OS),
                findings: vec![format!("host platform {} cannot run the {platform} tier", std::env::consts::OS)],
                runs: Vec::new(),
            });
        }
    }
    let crates = workspace_crates(root)?;
    let mut findings = Vec::new();
    let mut runs = Vec::new();
    for crate_name in &decl.core_crates {
        if !crates.contains(crate_name) {
            findings.push(format!(
                "core crate {crate_name} is not present in the workspace"
            ));
        }
    }
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for reg in file.verification.iter().filter(|r| r.tier == tier) {
        if reg.command.starts_with("cargo xtask verify --tier") {
            runs.push(Run {
                id: reg.id.clone(),
                command: reg.command.clone(),
                status: "self".into(),
                exit_code: None,
            });
            continue;
        }
        if let Some(target) = target_crate(&reg.command) {
            if !crates.contains(&target) {
                skipped += 1;
                findings.push(format!(
                    "{}: target crate {target} is not present yet; command not run",
                    reg.id
                ));
                runs.push(Run {
                    id: reg.id.clone(),
                    command: reg.command.clone(),
                    status: "skipped-missing-crate".into(),
                    exit_code: None,
                });
                continue;
            }
        }
        let parts: Vec<&str> = reg.command.split_whitespace().collect();
        let output = match Command::new(parts[0])
            .args(&parts[1..])
            .current_dir(root)
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                // The program that carries this verification is not installed on this host: the
                // verification is "not present", which strict mode refuses to accept.
                skipped += 1;
                findings.push(format!("{}: cannot run `{}`: {err}", reg.id, reg.command));
                runs.push(Run {
                    id: reg.id.clone(),
                    command: reg.command.clone(),
                    status: "skipped-missing-program".into(),
                    exit_code: None,
                });
                continue;
            }
        };
        let status = output.status;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !status.success()
            && (stderr.contains("no test target named")
                || stderr.contains("no bench target named")
                || stderr.contains("no example target named"))
        {
            skipped += 1;
            findings.push(format!(
                "{}: `{}` names a test target that does not exist yet",
                reg.id, reg.command
            ));
            runs.push(Run {
                id: reg.id.clone(),
                command: reg.command.clone(),
                status: "skipped-missing-test".into(),
                exit_code: status.code(),
            });
            continue;
        }
        let code = status.code();
        let stdout = String::from_utf8_lossy(&output.stdout);
        // `cargo test -p crate filter` exits 0 when the filter matches nothing. A named test that
        // does not exist yet is "not present", never a pass (contract-verification-tiering).
        let vacuous = status.success()
            && parts.first() == Some(&"cargo")
            && parts.get(1) == Some(&"test")
            && tests_run(&stdout) == 0;
        if vacuous {
            skipped += 1;
            findings.push(format!(
                "{}: `{}` ran zero tests; the named test is not present yet",
                reg.id, reg.command
            ));
            runs.push(Run {
                id: reg.id.clone(),
                command: reg.command.clone(),
                status: "skipped-missing-test".into(),
                exit_code: code,
            });
            continue;
        }
        if status.success() {
            runs.push(Run {
                id: reg.id.clone(),
                command: reg.command.clone(),
                status: "passed".into(),
                exit_code: code,
            });
        } else {
            std::io::Write::write_all(&mut std::io::stderr(), &output.stderr).ok();
            failed += 1;
            findings.push(format!(
                "{}: `{}` exited with {:?}",
                reg.id, reg.command, code
            ));
            runs.push(Run {
                id: reg.id.clone(),
                command: reg.command.clone(),
                status: "failed".into(),
                exit_code: code,
            });
        }
    }
    let missing_core = decl
        .core_crates
        .iter()
        .filter(|c| !crates.contains(*c))
        .count();
    let ok = failed == 0 && (!strict || (skipped == 0 && missing_core == 0));
    Ok(Report {
        ok,
        mode: format!("tier:{tier}{}", if strict { " (strict)" } else { "" }),
        summary: format!("{} run(s), {failed} failed, {skipped} skipped for absent crates, {missing_core} core crate(s) absent", runs.iter().filter(|r| r.status == "passed" || r.status == "failed").count()),
        findings,
        runs,
    })
}

/// Total tests executed across every `running N tests` header in cargo test output.
fn tests_run(stdout: &str) -> usize {
    stdout
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("running ")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|n| n.parse::<usize>().ok())
        })
        .sum()
}

pub fn print_text(report: &Report) {
    println!("verify {}: {}", report.mode, report.summary);
    for run in &report.runs {
        println!("  [{}] {} :: {}", run.status, run.id, run.command);
    }
    for f in &report.findings {
        println!("FINDING {f}");
    }
    println!("{}", if report.ok { "OK" } else { "FAILED" });
}
