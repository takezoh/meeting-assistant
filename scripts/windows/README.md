# Windows developer scripts

These PowerShell 5.1-compatible scripts provide a repeatable Windows developer
workflow for Meeting Assistant. Run them in Windows PowerShell or PowerShell on
Windows, from any directory.

## Quick start

If local policy prevents scripts from running, allow scripts only for the current
PowerShell process:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
```

Then, from the repository:

```powershell
.\scripts\windows\dev.ps1 bootstrap
.\scripts\windows\dev.ps1 verify
.\scripts\windows\dev.ps1 build -Configuration Release
.\scripts\windows\dev.ps1 install -Configuration Release
```

The bootstrap action:

- finds `rustup.exe` under `%USERPROFILE%\.cargo\bin` even when that directory is
  not yet on `PATH`;
- installs the stable Rust toolchain with `rustfmt` and `clippy`;
- detects the Visual Studio C++ x64 workload and, when needed, installs Visual
  Studio 2022 Build Tools through `winget`.

Preview bootstrap changes without installing anything:

```powershell
.\scripts\windows\dev.ps1 bootstrap -WhatIf
```

## Commands

```powershell
# Run formatting, architecture, registration, portable, workspace, clippy,
# headless UI, and Windows verification gates.
.\scripts\windows\dev.ps1 verify

# Build command-line processes and the Tauri UI.
.\scripts\windows\dev.ps1 build -Configuration Debug
.\scripts\windows\dev.ps1 build -Configuration Release

# Build without the Tauri UI.
.\scripts\windows\dev.ps1 build -SkipUi

# Build and install in one command.
.\scripts\windows\dev.ps1 install -Build -Configuration Release

# Run the full setup, verification, build, and developer-install flow.
.\scripts\windows\dev.ps1 all -Configuration Release
```

Build outputs are written to `target\debug` or `target\release`. The Tauri UI is
written to `target\ui\debug` or `target\ui\release`. Because Phase 1 does not yet
carry product branding assets, the developer build generates a minimal placeholder
ICO under `target\ui`; it is not a release icon.

## Developer installation

By default, `install` copies already-built executables to:

```text
%LOCALAPPDATA%\MeetingAssistant\dev\bin
```

It also writes `install-manifest.json` containing the source root, configuration,
timestamp, and SHA-256 digest of every installed executable. Use `-AddToPath` only
when you explicitly want that directory appended to the user `PATH`:

```powershell
.\scripts\windows\dev.ps1 install -Configuration Release -AddToPath
```

To build immediately before copying, pass `-Build`:

```powershell
.\scripts\windows\dev.ps1 install -Build -Configuration Release
```

This is a developer installation, not a signed product installer. It does not
write to Program Files, register an uninstaller, configure automatic startup, or
change browser policy.
