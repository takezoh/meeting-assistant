//! Tests that read the source code to confirm the responsibility boundaries have not eroded.
//!
//! The rules "modules that make decisions have no side effects" and "OS APIs are used only inside `platform`"
//! are not enforced by the compiler, so they are enforced here.

use std::path::{Path, PathBuf};

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The path relative to `src` (separated by `/`) and the contents with test code removed.
fn sources() -> Vec<(String, String)> {
    fn collect(directory: &Path, root: &Path, found: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, root, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap();
                found.push((relative, production(&text)));
            }
        }
    }

    let root = source_root();
    let mut found = Vec::new();
    collect(&root, &root, &mut found);
    assert!(found.len() > 10, "no source files found");
    found
}

/// The part of a source file that is built into the app. Tests may use threads and temporary files to drive fakes, so
/// they are left out: the test modules at the end of a file, from the first `#[cfg(test)]` that opens a module there,
/// and a test-only module declaration such as `mod architecture;`. Anything else marked `#[cfg(test)]` is scanned.
fn production(text: &str) -> String {
    let mut kept = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line == "#[cfg(test)]"
            && let Some(next) = lines.peek()
        {
            if next.trim_end().ends_with(';') {
                lines.next();
                continue;
            }
            if next.starts_with("mod ") || next.starts_with("pub mod ") {
                break;
            }
        }
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

#[test]
fn test_code_is_left_out_of_the_scan_but_production_code_after_it_is_not() {
    let text = "mod a;\n#[cfg(test)]\nmod architecture;\nfn main() { std::thread::spawn(f); }\n\
                impl X {\n    #[cfg(test)]\n    fn len(&self) {}\n    fn later() {}\n}\n\
                #[cfg(test)]\nmod tests {\n    use std::fs;\n}\n";

    let kept = production(text);

    assert!(kept.contains("std::thread::spawn"), "{kept}");
    assert!(kept.contains("fn later"), "{kept}");
    assert!(
        !kept.contains("mod architecture") && !kept.contains("std::fs"),
        "{kept}"
    );
}

fn violations(files: &[(String, String)], is_target: impl Fn(&str) -> bool, forbidden: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, text) in files.iter().filter(|(path, _)| is_target(path)) {
        for token in forbidden {
            if text.contains(token) {
                found.push(format!("{path} uses {token}"));
            }
        }
    }
    found
}

#[test]
fn os_apis_are_used_only_inside_platform() {
    let os_specific = [
        "windows::",
        "windows_core::",
        "libc::",
        "signal_hook::",
        "notify_rust::",
        "window_vibrancy::",
        "tao::platform::",
        "objc2",
        "block2::",
        "dispatch2::",
    ];
    let files = sources();

    let found = violations(
        &files,
        |path| !path.starts_with("platform/") && path != "architecture.rs",
        &os_specific,
    );

    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn decision_modules_have_no_side_effects() {
    // They do not touch files, networking, other processes, the environment, or OS implementations.
    // Anything they need comes in through a port (trait).
    let side_effects = [
        "std::fs",
        "std::net",
        "std::process",
        "std::env",
        "crate::platform",
        "crate::adapters",
    ];
    let pure = [
        "cli.rs",
        "bundle.rs",
        "install/mod.rs",
        "linked.rs",
        "panel/view.rs",
        "worker.rs",
        "browser/protocol.rs",
        "browser/hub.rs",
        "browser/peer.rs",
        "recording/mod.rs",
        "recording/merge.rs",
        "recording/mp4.rs",
        "recording/fit.rs",
        "recording/timeline.rs",
        "recording/mixer.rs",
        "recording/pcm.rs",
        "recording/plan.rs",
        "recording/pipeline.rs",
    ];
    let files = sources();
    for path in pure {
        assert!(
            files.iter().any(|(found, _)| found == path),
            "{path} not found (update the list)"
        );
    }

    let found = violations(&files, |path| pure.contains(&path), &side_effects);

    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn time_and_audio_calculations_touch_neither_clock_nor_threads() {
    let clocks_and_threads = ["Instant::now", "SystemTime", "std::thread", "chrono::"];
    let files = sources();

    let found = violations(
        &files,
        |path| {
            (path.starts_with("recording/") && path != "recording/session.rs")
                || matches!(
                    path,
                    "cli.rs" | "install/mod.rs" | "browser/hub.rs" | "linked.rs" | "panel/view.rs"
                )
        },
        &clocks_and_threads,
    );

    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn every_os_implementation_exposes_the_same_names() {
    let files = sources();
    let text = |path: &str| {
        files
            .iter()
            .find(|(found, _)| found == path)
            .map(|(_, text)| text.as_str())
            .unwrap()
    };
    // Names re-exported by platform/mod.rs.
    let exported: Vec<&str> = text("platform/mod.rs")
        .split("pub use self::imp::{")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap()
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    assert!(exported.len() >= 8, "{exported:?}");

    for implementation in ["platform/win/mod.rs", "platform/mac/mod.rs"] {
        for name in &exported {
            assert!(
                text(implementation).contains(name),
                "{implementation} does not expose {name}"
            );
        }
    }
}
