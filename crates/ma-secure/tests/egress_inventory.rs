//! The egress inventory check (contract-egress-inventory): every host reachable from workspace
//! source or from a `contracts/` manifest is declared in `egress-inventory.toml`, every entry
//! declares a closed `integration_owner`, an active entry that nothing references is stale, and
//! an undeclared host fails naming file, line and host.

use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Inventory {
    #[serde(default)]
    exclude_paths: Vec<String>,
    #[serde(default)]
    host: Vec<Entry>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Entry {
    host: String,
    component: String,
    purpose: String,
    integration_owner: String,
    credential_kind: String,
}

const OWNERS: [&str; 3] = ["user_account", "distribution", "operating_system"];
/// Reserved names that are never an outbound product host.
const RESERVED_SUFFIXES: [&str; 6] = [
    ".test",
    ".example",
    ".invalid",
    ".localhost",
    ".local",
    ".internal",
];
/// A bare dotted literal is a host only when its last label is a public top-level domain; dotted
/// identifiers such as `policy.evaluate` are not hosts. A literal carrying a URL scheme is always a host.
const PUBLIC_TLDS: [&str; 14] = [
    "com", "net", "org", "io", "dev", "app", "ai", "jp", "co", "uk", "us", "cloud", "ms", "me",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    code: &'static str,
    detail: String,
}

#[derive(Debug, Default)]
struct Report {
    findings: Vec<Finding>,
    referenced: BTreeMap<String, Vec<String>>,
}

fn is_excluded(rel: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|pat| {
        let p = pat.trim_end_matches("/**").trim_end_matches('/');
        rel == p || rel.starts_with(&format!("{p}/"))
    })
}

fn host_candidates(text: &str) -> Vec<(usize, String)> {
    // string literals that parse as a hostname or a URL with a hostname
    let literal = Regex::new(r#""([^"\\]|\\.)*""#).unwrap();
    let host = Regex::new(r"^(?:[a-z][a-z0-9+.-]*://)?([a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?)+)(?:[:/].*)?$").unwrap();
    let mut out = Vec::new();
    // Skip only an inline `#[cfg(test)] mod x { ... }` block, tracked by brace depth; production
    // code after the block (or a `#[cfg(test)] mod tests;` declaration) is scanned like any other.
    let mut pending_cfg_test = false;
    let mut test_depth: Option<usize> = None;
    let mut depth: usize = 0;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            pending_cfg_test = true;
            continue;
        }
        let opens = line.matches('{').count();
        let closes = line.matches('}').count();
        if pending_cfg_test {
            if trimmed.starts_with("mod ") && opens > 0 {
                test_depth = Some(depth);
            }
            if !trimmed.starts_with('#') {
                pending_cfg_test = false;
            }
        }
        depth += opens;
        let inside_tests = test_depth.is_some();
        depth = depth.saturating_sub(closes);
        if let Some(d) = test_depth {
            if depth <= d && closes > 0 {
                test_depth = None;
            }
        }
        if inside_tests {
            continue;
        }
        for m in literal.find_iter(line) {
            let inner = &m.as_str()[1..m.as_str().len() - 1];
            if let Some(c) = host.captures(inner.trim()) {
                let h = c[1].to_string();
                let has_scheme = inner.contains("://");
                let tld = h.rsplit('.').next().unwrap_or_default();
                if !has_scheme && !PUBLIC_TLDS.contains(&tld) {
                    continue;
                }
                if RESERVED_SUFFIXES.iter().any(|s| h.ends_with(s))
                    || !h.contains('.')
                    || h.ends_with(".rs")
                    || h.ends_with(".toml")
                    || h.ends_with(".json")
                    || h.ends_with(".md")
                    || h.ends_with(".wav")
                    || h.ends_with(".flac")
                    || h.ends_with(".exe")
                    || h.ends_with(".db")
                    || h.ends_with(".sql")
                {
                    continue;
                }
                out.push((index + 1, h));
            }
        }
    }
    out
}

fn walk(root: &Path, dir: &Path, excludes: &[String], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if is_excluded(&rel, excludes) || rel.starts_with("target") || rel.starts_with(".git") {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, excludes, out);
        } else if rel.ends_with(".rs")
            || (rel.starts_with("contracts/") && (rel.ends_with(".json") || rel.ends_with(".toml")))
        {
            out.push(path);
        }
    }
}

fn check(workspace_root: &Path, inventory_path: &Path) -> Report {
    let mut report = Report::default();
    let text = std::fs::read_to_string(inventory_path).expect("inventory readable");
    let inventory: Inventory = toml::from_str(&text).expect("inventory parses");
    let declared: BTreeMap<String, Entry> = inventory
        .host
        .iter()
        .map(|e| (e.host.clone(), e.clone()))
        .collect();
    for entry in &inventory.host {
        if !OWNERS.contains(&entry.integration_owner.as_str()) {
            report.findings.push(Finding {
                code: "owner_not_closed",
                detail: format!(
                    "{}: integration_owner {:?} is not one of {OWNERS:?}",
                    entry.host, entry.integration_owner
                ),
            });
        }
        if entry.component.is_empty()
            || entry.purpose.is_empty()
            || entry.credential_kind.is_empty()
        {
            report.findings.push(Finding {
                code: "entry_incomplete",
                detail: format!(
                    "{}: component, purpose and credential_kind are required",
                    entry.host
                ),
            });
        }
    }
    let mut files = Vec::new();
    walk(
        workspace_root,
        workspace_root,
        &inventory.exclude_paths,
        &mut files,
    );
    for file in files {
        let rel = file
            .strip_prefix(workspace_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let candidates = if rel.starts_with("contracts/") {
            manifest_hosts(&text)
        } else {
            host_candidates(&text)
        };
        for (line, host) in candidates {
            report
                .referenced
                .entry(host.clone())
                .or_default()
                .push(format!("{rel}:{line}"));
            if !declared.contains_key(&host) {
                report.findings.push(Finding {
                    code: "undeclared_host",
                    detail: format!(
                        "{rel}:{line}: host {host} is not declared in egress-inventory.toml"
                    ),
                });
            }
        }
    }
    for entry in &inventory.host {
        if !report.referenced.contains_key(&entry.host) {
            report.findings.push(Finding {
                code: "stale_entry",
                detail: format!(
                    "{}: declared active but reachable from no source or manifest",
                    entry.host
                ),
            });
        }
    }
    report
}

/// `egress_hosts` arrays in processor and destination manifests.
fn manifest_hosts(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_hosts = false;
    let quoted_host = Regex::new(r#""([a-z0-9.-]+\.[a-z]{2,})""#).unwrap();
    for (index, line) in text.lines().enumerate() {
        if line.contains("egress_hosts") {
            in_hosts = true;
        }
        if in_hosts {
            for host in quoted_host.captures_iter(line) {
                out.push((index + 1, host[1].to_string()));
            }
            if line.contains(']') {
                in_hosts = false;
            }
        }
    }
    out
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn every_source_host_is_declared() {
    let root = workspace_root();
    let report = check(&root, &root.join("egress-inventory.toml"));
    let undeclared: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "undeclared_host")
        .collect();
    assert!(undeclared.is_empty(), "undeclared hosts: {undeclared:?}");
}

#[test]
fn owners_are_closed_and_entries_are_reachable() {
    let root = workspace_root();
    let report = check(&root, &root.join("egress-inventory.toml"));
    let bad: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code != "undeclared_host")
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    let text = std::fs::read_to_string(root.join("egress-inventory.toml")).unwrap();
    let inventory: Inventory = toml::from_str(&text).unwrap();
    assert!(
        inventory
            .host
            .iter()
            .all(|e| OWNERS.contains(&e.integration_owner.as_str())),
        "every owner is one of the three closed owners; first_party cannot be expressed"
    );
    let hosts: BTreeSet<&str> = inventory.host.iter().map(|e| e.host.as_str()).collect();
    for expected in [
        "oauth2.googleapis.com",
        "www.googleapis.com",
        "api.notion.com",
        "api.openai.com",
        "api.anthropic.com",
        "objects.githubusercontent.com",
    ] {
        assert!(hosts.contains(expected), "{expected} is an inventory host");
    }
}

#[test]
fn undeclared_host_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("crates/ma-destination-telemetry/src")).unwrap();
    std::fs::write(root.join("crates/ma-destination-telemetry/src/lib.rs"), "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        let _ = \"https://tests-only.example-vendor.com/\";\n    }\n}\npub const ENDPOINT: &str = \"https://telemetry.example-vendor.com/v1/ping\";\npub const OK: &str = \"api.notion.com\";\n").unwrap();
    std::fs::write(root.join("egress-inventory.toml"), "exclude_paths = []\n[[host]]\nhost = \"api.notion.com\"\ncomponent = \"ma-destination-notion\"\npurpose = \"export\"\nintegration_owner = \"user_account\"\ncredential_kind = \"internal_integration_token\"\n[[host]]\nhost = \"dead.example-vendor.com\"\ncomponent = \"none\"\npurpose = \"none\"\nintegration_owner = \"user_account\"\ncredential_kind = \"none\"\n").unwrap();
    let report = check(root, &root.join("egress-inventory.toml"));
    let undeclared: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "undeclared_host")
        .collect();
    assert_eq!(undeclared.len(), 1, "{:?}", report.findings);
    assert!(
        undeclared[0]
            .detail
            .contains("telemetry.example-vendor.com")
            && undeclared[0]
                .detail
                .contains("crates/ma-destination-telemetry/src/lib.rs:8"),
        "{}",
        undeclared[0].detail
    );
    let stale: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "stale_entry")
        .collect();
    assert_eq!(stale.len(), 1, "a stale entry is a distinct failure code");
    assert!(stale[0].detail.contains("dead.example-vendor.com"));
    // a first_party owner cannot be expressed
    std::fs::write(root.join("egress-inventory.toml"), "[[host]]\nhost = \"api.notion.com\"\ncomponent = \"x\"\npurpose = \"x\"\nintegration_owner = \"first_party\"\ncredential_kind = \"none\"\n").unwrap();
    let report = check(root, &root.join("egress-inventory.toml"));
    assert!(report.findings.iter().any(|f| f.code == "owner_not_closed"));
    // nor can an entry exempt itself from the reachability rule with an extra field
    std::fs::write(root.join("egress-inventory.toml"), "[[host]]\nhost = \"api.notion.com\"\ncomponent = \"x\"\npurpose = \"x\"\nintegration_owner = \"user_account\"\ncredential_kind = \"none\"\nstatus = \"planned\"\n").unwrap();
    let parsed = std::panic::catch_unwind(|| check(root, &root.join("egress-inventory.toml")));
    assert!(
        parsed.is_err(),
        "an entry with a status field is not a valid inventory entry"
    );
}
