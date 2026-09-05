//! `cargo xtask` entry point: `boundary` enforces boundary.toml, `verify` runs and checks
//! verification tiers (contract-module-boundary-enforcement, contract-verification-tiering).

mod boundary;
mod docs_check;
mod manual_record;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  cargo xtask boundary [--workspace DIR] [--rule NAME] [--check forbidden-imports] [--format json]\n  cargo xtask verify --check-registration [--workspace DIR] [--plan FILE] [--tiers FILE] [--format json]\n  cargo xtask verify --tier portable|windows [--strict] [--workspace DIR] [--tiers FILE] [--format json]\n  cargo xtask docs-check [--rule adr-placement|design-set|change-members|promotion-none|schema] [--workspace DIR] [--format json]\n  cargo xtask manual-record --id ID [--require pass|fail|blocked] [--manifest FILE] [--workspace DIR] [--format json]"
    );
    ExitCode::from(2)
}

fn workspace_root(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir;
    }
    // `cargo run` sets CARGO_MANIFEST_DIR to xtask/; the workspace root is its parent.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or(manifest_dir)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };
    let mut workspace = None;
    let mut rule = None;
    let mut check = None;
    let mut format_json = false;
    let mut tier = None;
    let mut registration = false;
    let mut strict = false;
    let mut plan = None;
    let mut tiers = None;
    let mut id = None;
    let mut require = String::from("pass");
    let mut manifest = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workspace" => {
                workspace = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--id" => {
                id = args.get(i + 1).cloned();
                i += 1;
            }
            "--require" => {
                require = args.get(i + 1).cloned().unwrap_or_default();
                i += 1;
            }
            "--manifest" => {
                manifest = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--rule" => {
                rule = args.get(i + 1).cloned();
                i += 1;
            }
            "--check" => {
                check = args.get(i + 1).cloned();
                i += 1;
            }
            "--tier" => {
                tier = args.get(i + 1).cloned();
                i += 1;
            }
            "--plan" => {
                plan = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--tiers" => {
                tiers = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--format" => {
                format_json = args.get(i + 1).map(|v| v == "json").unwrap_or(false);
                i += 1;
            }
            "--check-registration" => registration = true,
            "--strict" => strict = true,
            other => {
                eprintln!("unknown argument: {other}");
                return usage();
            }
        }
        i += 1;
    }
    let root = workspace_root(workspace);
    match command.as_str() {
        "boundary" => {
            if check.as_deref().is_some_and(|c| c != "forbidden-imports") {
                eprintln!("unknown --check value; only forbidden-imports is defined");
                return usage();
            }
            match boundary::check(&root, &boundary::Options { rule, check }) {
                Ok(report) => {
                    if format_json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).expect("report is serializable")
                        );
                    } else {
                        boundary::print_text(&report);
                    }
                    if report.violations.is_empty() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(1)
                    }
                }
                Err(err) => {
                    eprintln!("boundary check failed to run: {err}");
                    ExitCode::from(2)
                }
            }
        }
        "manual-record" => {
            let Some(id) = id else {
                eprintln!("manual-record needs --id");
                return usage();
            };
            if !matches!(require.as_str(), "pass" | "fail" | "blocked") {
                eprintln!("--require must be pass, fail or blocked");
                return usage();
            }
            match manual_record::check(&root, manifest.as_deref(), &id, &require) {
                Ok(report) => {
                    if format_json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).expect("report is serializable")
                        );
                    } else {
                        manual_record::print_text(&report);
                    }
                    if report.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(1)
                    }
                }
                Err(err) => {
                    eprintln!("manual-record failed to run: {err}");
                    ExitCode::from(2)
                }
            }
        }
        "docs-check" => match docs_check::run(&root, rule.as_deref()) {
            Ok(report) => {
                if format_json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).expect("report is serializable")
                    );
                } else {
                    docs_check::print_text(&report);
                }
                if report.findings.is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(err) => {
                eprintln!("docs check failed to run: {err}");
                ExitCode::from(2)
            }
        },
        "verify" => {
            let outcome = if registration {
                verify::check_registration(&root, plan.as_deref(), tiers.as_deref())
            } else if let Some(tier) = tier {
                verify::run_tier(&root, &tier, tiers.as_deref(), strict)
            } else {
                return usage();
            };
            match outcome {
                Ok(report) => {
                    if format_json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).expect("report is serializable")
                        );
                    } else {
                        verify::print_text(&report);
                    }
                    if report.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(1)
                    }
                }
                Err(err) => {
                    eprintln!("verify failed to run: {err}");
                    ExitCode::from(2)
                }
            }
        }
        _ => usage(),
    }
}
