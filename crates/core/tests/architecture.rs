//! A test that reads the source code to verify that core does not touch the outside world directly.
//!
//! core does not use files, the clock, networking, or OS APIs. What it needs is received through ports (traits).
//! The compiler does not enforce this rule, so it is enforced here.

use std::path::Path;

/// File names and contents excluding test code.
fn sources() -> Vec<(String, String)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let production = text.split("#[cfg(test)]").next().unwrap().to_owned();
        found.push((path.file_name().unwrap().to_string_lossy().into_owned(), production));
    }
    assert!(found.len() >= 8, "could not find the sources");
    found
}

#[test]
fn core_does_not_touch_the_outside_world_directly() {
    let forbidden = [
        "std::fs",
        "std::net",
        "std::process",
        "std::env",
        "std::thread",
        "Instant::now",
        "SystemTime",
        "chrono::",
        "windows::",
        "libc::",
    ];

    let mut violations = Vec::new();
    for (file, text) in sources() {
        for token in forbidden {
            if text.contains(token) {
                violations.push(format!("{file} uses {token}"));
            }
        }
    }

    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn core_does_not_depend_on_os_specific_crates() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();

    for forbidden in ["windows", "libc", "chrono", "dirs", "tokio"] {
        assert!(
            !dependencies
                .lines()
                .any(|line| line.trim_start().starts_with(forbidden)),
            "core depends on {forbidden}"
        );
    }
}
