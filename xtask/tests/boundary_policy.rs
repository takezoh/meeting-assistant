//! Completeness guard for the capture-path-isolation rule (contract-capture-path-isolation-scope).
//!
//! `cargo xtask boundary --rule capture-path-isolation` passes vacuously when the rule's `sources`
//! list is shorter than the capture path the design documents describe: an unlisted crate is simply
//! not scanned. module-boundaries.md INV-002 reads over the whole capture path, so the list itself
//! is asserted here and shortening it is a test failure rather than a silently smaller scan.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every crate module-boundaries.md calls a capture-path crate: the L0 core types, the L2 session
/// authority, and the three L3 crates that observe or capture on the recording side.
const CAPTURE_PATH_CRATES: &[&str] = &[
    "ma-core-types",
    "ma-session",
    "ma-capture",
    "ma-signals-windows",
    "ma-ext-channel",
];

#[test]
fn capture_path_isolation_names_every_capture_path_crate() {
    let policy_path = repo_root().join("boundary.toml");
    let text = std::fs::read_to_string(&policy_path).expect("boundary.toml is readable");
    let policy: toml::Value = toml::from_str(&text).expect("boundary.toml parses");
    let sources: BTreeSet<String> = policy["rules"]["capture-path-isolation"]["sources"]
        .as_array()
        .expect("[rules.capture-path-isolation].sources is an array")
        .iter()
        .map(|v| v.as_str().expect("source names are strings").to_string())
        .collect();
    let expected: BTreeSet<String> = CAPTURE_PATH_CRATES.iter().map(|s| s.to_string()).collect();
    let missing: Vec<&String> = expected.difference(&sources).collect();
    assert!(
        missing.is_empty(),
        "capture-path-isolation sources {:?} omit capture-path crate(s) {:?} named by module-boundaries.md INV-002",
        sources,
        missing
    );
    for name in &sources {
        assert!(
            repo_root()
                .join("crates")
                .join(name)
                .join("Cargo.toml")
                .is_file(),
            "capture-path-isolation source {name} is not a workspace crate"
        );
    }
}
