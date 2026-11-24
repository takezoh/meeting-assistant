---
id: issue-20261124-the-windows-installer-is-not-built
kind: issue
title: The Windows installer is designed but not built
status: open
created: '2026-11-24'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: adr-20261124-distribute-a-built-app-that-installs-itself}
subject_paths:
- crates/app/src/platform/win/install.rs
- crates/app/src/platform/win/mod.rs
- packaging
summary: The decision to distribute a built app that installs itself covers Windows,
  but only macOS has a build script and the offer to install when opened. On Windows
  the executable is still built from source and installed with --install or its tray
  menu.
---

## Opportunity

[The decision](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md) makes the Windows release the executable itself. Downloaded and opened, it asks whether to install, and installs into `%LOCALAPPDATA%\Programs\MeetingAssistant` through the shared procedure. None of this is built for Windows yet.

## Evidence

As of 2026-11-24:

- `PlatformInstaller::installs_when_opened` returns `false` on Windows (`crates/app/src/platform/win/install.rs`). An executable that is not installed therefore runs where it is and offers the installation in its tray menu.
- `confirm` shows Windows' OK and Cancel message box (`crates/app/src/platform/win/mod.rs`). It is type-checked and has never run.
- There is no `packaging/windows`. The executable has no version information or icon in its resources.

## Context for follow-up

The steps the decision implies:

1. **Resources.** Embed version information (product name, version from `CARGO_PKG_VERSION`), an icon and an application manifest. Explorer, Settings > Apps and SmartScreen then show a name instead of a bare file.
2. **Asking when opened.** Make `installs_when_opened` true. A development build then asks too, unless it runs with `--config`, as [the development guide](../development.md) already says to do.
3. **Build script.** Add `packaging/windows/build.ps1`. It runs `cargo build --release --locked` and copies the executable to `target\windows\MeetingAssistant-<version>.exe`. When a certificate is named, it signs with `signtool sign /fd SHA256 /tr <timestamp URL> /td SHA256`. The installer copies the file to the fixed name `meeting-assistant.exe`, so the download may have any name.
4. **Checks on Windows 11:**
   - Install from a downloaded copy, with SmartScreen's warning for an unsigned file.
   - Update over a running recording: the old copy must finalize it before it is replaced.
   - Cancel: nothing changes.
   - Uninstall from Settings > Apps.

Not part of it: an MSI, an MSIX package, or a wizard toolchain. The decision lists why.
