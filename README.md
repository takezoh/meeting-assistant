# Meeting Assistant

Meeting Assistant is a desktop-first application for detecting supported meetings,
recording audio locally, processing the recording, and exporting user-owned
artifacts without placing a proprietary backend in the workflow data path.

The target platform is Windows 11. The MVP is intended to support Microsoft Teams,
Slack Huddles, Google Meet, and Zoom without joining a meeting as a bot or relying
on DOM scraping, accessibility-tree layouts, private APIs, or UI coordinates.

## Project status

This repository is a developer prototype, not a production-ready application.

- Phase 0 defines the repository structure, contracts, security boundaries,
  session model, artifact model, and automated dependency rules.
- Phase 1 implements the Windows detection and audio-capture proof of concept,
  diagnostic tooling, replayable signal fixtures, and a detection-only browser
  extension.
- Product session orchestration, transcription, summarization, destination
  integrations, release packaging, and code signing belong to later phases.

The current `install` script creates a user-local developer installation. It is
not a signed Windows installer and does not configure automatic startup.

## What is implemented

- Windows process, package-identity, audio-session, and microphone-use observation.
- Process-specific WASAPI loopback capture with documented fallback behavior.
- Microphone endpoint selection that follows the meeting application's active
  capture session.
- A deterministic detector that correlates browser tab and microphone evidence by
  process-tree identity.
- A diagnostic `ma-diag` command for recording, labeling, replaying, and measuring
  captured sessions.
- Redacted, replayable signal timelines for Teams, Slack, Meet, and Zoom scenarios.
- A Manifest V3 browser extension proof of concept that reports only tab hostname
  and audible state over an authenticated loopback channel.
- Mechanically enforced crate layering, capture-path isolation, verification
  registration, and portable/Windows test tiers.

## Windows quick start

Open Windows PowerShell in the repository. If script execution is restricted, allow
it only for the current process:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
```

Preview prerequisite changes:

```powershell
.\scripts\windows\dev.ps1 bootstrap -WhatIf
```

Run the complete developer workflow:

```powershell
.\scripts\windows\dev.ps1 all -Configuration Release
```

The workflow detects or installs Rust and the Visual Studio C++ x64 workload, runs
the repository verification gates, builds the executables, and copies them to:

```text
%LOCALAPPDATA%\MeetingAssistant\dev\bin
```

Individual steps are also available:

```powershell
.\scripts\windows\dev.ps1 bootstrap
.\scripts\windows\dev.ps1 verify
.\scripts\windows\dev.ps1 build -Configuration Release
.\scripts\windows\dev.ps1 install -Configuration Release
```

See [scripts/windows/README.md](scripts/windows/README.md) for options, output paths,
the generated developer icon, and PATH configuration.

## Portable development checks

Rust stable with `rustfmt` and `clippy` is required. The portable CI-equivalent
checks are:

```bash
cargo fmt --all -- --check
cargo xtask boundary
cargo xtask verify --check-registration
cargo xtask verify --tier portable --strict
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path app/ui/src-tauri/Cargo.toml --no-default-features --locked
```

The native Windows tier must run on Windows:

```powershell
cargo xtask verify --tier windows --strict
```

Do not treat a portable or GNU cross-target check as proof of native Windows API,
audio-device, ACL, or application behavior.

## Repository layout

| Path | Purpose |
| --- | --- |
| `crates/` | Layered Rust libraries and process entry points |
| `app/ui/src-tauri/` | Tauri consent and visibility shell, outside the root workspace |
| `contracts/` | Stable schemas and behavioral contracts |
| `extension/` | Detection-only browser extension proof of concept |
| `fixtures/signal-timelines/` | Redacted detector replay fixtures and labels |
| `xtask/` | Architecture, documentation, registration, and tier verification tools |
| `docs/design/` | Active architecture and security invariants |
| `docs/adr/` | Accepted architecture decisions |
| `docs/changes/` | Requirements, implementation records, tasks, and verification evidence |
| `scripts/windows/` | Windows bootstrap, verification, build, and developer-install scripts |

The crate dependency model is defined in `boundary.toml` and enforced by
`cargo xtask boundary`. The main layers are:

```text
L5  composition roots and tools
L4  meeting-service adapters
L3  storage, capture, Windows signals, extension channel
L2  session, detector, workflow
L1  contracts and security primitives
L0  core types
```

Dependencies normally point toward lower layers. L4 is intentionally a sink: only
L5 composition roots may depend on service-specific adapters.

## Design and security principles

- Recording artifacts and workflow state remain local unless the user configures
  an export destination.
- Meeting-service-specific behavior stays behind thin adapters.
- Browser detection uses public extension APIs and never reads page content.
- The extension signal is non-authoritative and cannot start recording by itself.
- Secrets and meeting content use restricted carrier types and must not enter logs.
- Every network destination must be declared in `egress-inventory.toml`.
- Capture-path crates cannot depend on processing, destination, storage, or adapter
  implementations.

Start with [PLAN.md](PLAN.md), then read the active documents under
[`docs/design/`](docs/design/) and the relevant change package under
[`docs/changes/`](docs/changes/) before modifying a subsystem.

## Browser extension proof of concept

The extension is intentionally detection-only. It has no content script, DOM
access, tab-audio capture, or broad host permissions. The diagnostic harness
generates its untracked endpoint configuration at runtime.

See [extension/README.md](extension/README.md) for provisioning, loading, protocol,
and security details.
