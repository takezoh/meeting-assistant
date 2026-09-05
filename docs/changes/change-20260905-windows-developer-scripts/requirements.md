---
change: change-20260905-windows-developer-scripts
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Developer workflow requirements

- WDS-001: `bootstrap.ps1` shall run from ordinary Windows PowerShell 5.1 or newer,
  detect an existing rustup installation even when PATH is stale, and install stable
  Rust with rustfmt and clippy when absent.
- WDS-002: Bootstrap shall detect the MSVC x64 C++ workload and use winget to install
  it when absent, with actionable failure messages when automatic installation is
  unavailable.
- WDS-003: `build.ps1` shall resolve the repository root independently of the caller's
  current directory and build the engine, diagnostic harness, processor host, manifest
  signer, and optional Tauri UI in Debug or Release configuration.
- WDS-004: `verify.ps1` shall expose the repository's formatting, boundary,
  registration, test, clippy, and optional Windows-tier gates.
- WDS-005: `install.ps1` shall copy an existing build into a per-user development
  location, emit a hash-bearing install manifest, and modify user PATH only when
  explicitly requested.
- WDS-006: `dev.ps1` shall provide one discoverable dispatcher for bootstrap, verify,
  build, install, and all-in-one setup.
- WDS-007: Scripts shall stop on failed native commands and avoid claiming that this
  developer install is the signed production installer.

## Exclusions

No code signing, MSI/MSIX/NSIS bundle, Program Files installation, service/logon task,
browser managed policy, or production update registration is introduced.
