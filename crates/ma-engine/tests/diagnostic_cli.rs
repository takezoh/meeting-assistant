//! Process-level checks for NFR-105: only an explicit `record` invocation may start capture.

use std::process::Command;

#[test]
fn diagnostic_harness_requires_explicit_invocation() {
    let binary = env!("CARGO_BIN_EXE_ma-diag");
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("artifacts");

    let no_command = Command::new(binary).status().unwrap();
    assert_eq!(no_command.code(), Some(2));
    assert!(!artifacts.exists());

    let list = Command::new(binary).arg("list").status().unwrap();
    assert!(list.success());
    assert!(!artifacts.exists());

    let record = Command::new(binary)
        .args(["record", "--artifact-root"])
        .arg(&artifacts)
        .env_remove("MA_EXTENSION_ID")
        .env_remove("MA_OWNER_SID")
        .status()
        .unwrap();
    assert_eq!(record.code(), Some(if cfg!(windows) { 2 } else { 4 }));
    assert!(
        !artifacts.exists(),
        "a rejected record invocation must not create artifacts"
    );
}
