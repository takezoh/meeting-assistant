---
change: change-20260905-windows-developer-scripts
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

## Discriminating checks

- Parse every `.ps1` with the Windows PowerShell parser.
- Exercise `dev.ps1 help` and parameter validation without installing dependencies.
- Confirm source inspection covers failure propagation, stale cargo PATH recovery,
  MSVC workload detection, explicit PATH opt-in, SHA-256 manifests, and repository-root
  independence.
- Run repository Rust gates after adding the scripts.

## Mechanical gates

- PASS (2026-09-05): PowerShell 7.4.6 parser accepted every
  `scripts/windows/*.ps1`; `dev.ps1 help` exited zero. The scripts declare
  `#Requires -Version 5.1` and use no post-5.1 language operators.
- PASS (2026-09-05): `cargo fmt --all -- --check`.
- PASS (2026-09-05): `cargo test --workspace`.
- PASS (2026-09-05): `cargo clippy --workspace --all-targets -- -D warnings`.
- PASS (2026-09-05): `cargo xtask boundary`.
- PASS (2026-09-05): `cargo xtask verify --check-registration`.
- PASS (2026-09-05): `cargo xtask verify --tier portable --strict` (127 runs).
- PASS (2026-09-05): `cargo test --manifest-path
  app/ui/src-tauri/Cargo.toml --no-default-features --locked` (4 tests).
- PASS (2026-09-05): `cargo check --workspace --all-targets --target
  x86_64-pc-windows-gnu`.
- PASS (2026-09-05): Tauri UI Windows-target check with the same generated
  developer ICO and `TAURI_CONFIG` override used by `build.ps1`.
- PASS (2026-09-05): `docs lint` before implementation.

## Remaining native smoke test

This Linux environment does not expose Windows PowerShell 5.1, `winget`, Visual
Studio Installer, or an MSVC linker. A Windows host must still run:

```powershell
.\scripts\windows\dev.ps1 bootstrap -WhatIf
.\scripts\windows\dev.ps1 bootstrap
.\scripts\windows\dev.ps1 all -Configuration Release
```

Do not record native bootstrap/build/install as verified until those commands have
completed on Windows.
