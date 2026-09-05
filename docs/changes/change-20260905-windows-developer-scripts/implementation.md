---
change: change-20260905-windows-developer-scripts
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

## Script topology

- `common.ps1`: repository-root discovery, native-command failure propagation, Rust
  PATH recovery, configuration and artifact-path helpers.
- `bootstrap.ps1`: rustup and Visual Studio Build Tools acquisition/detection.
- `verify.ps1`: CI-equivalent portable checks plus opt-in Windows T2 checks.
- `build.ps1`: selected configuration build with optional verification and UI.
- `install.ps1`: user-local copy and SHA-256 manifest, with opt-in PATH update.
- `dev.ps1`: dispatcher and all-in-one orchestration.
- `README.md`: execution-policy note, examples, outputs, and production-installer
  boundary.

All mutating acquisition operations support PowerShell `-WhatIf`. Build and install
consume only repository-relative paths computed from the scripts' physical location.
