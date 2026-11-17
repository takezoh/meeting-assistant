---
id: design-installation
kind: design
title: Per-user installation and lifecycle
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: "Install, update and uninstall the app for the current user: files, start at logon, native-host registration, browser launchers, uninstall entry."
- id: RESP-002
  statement: "Ensure a single resident instance and let another invocation ask it to finalize and exit."
- id: RESP-003
  statement: "Finalize a recording before the process ends on quit, logoff, shutdown or a termination signal."
invariants:
- id: INV-001
  statement: "Installation changes only the current user's profile and needs no elevation."
  enforcement: review
- id: INV-002
  statement: "If the running instance cannot be made to exit, install and uninstall change nothing."
  enforcement: test
- id: INV-003
  statement: "An installation that fails part-way stops at the failure and does not start the app."
  enforcement: test
- id: INV-004
  statement: "Uninstall removes every registration before removing files and keeps configuration, logs and recordings."
  enforcement: test
- id: INV-005
  statement: "The native-host manifest names the installed executable and only this extension's origin."
  enforcement: test
- id: INV-006
  statement: "Native hosts and launchers are created only for browsers that are present."
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: "The commands --install, --uninstall and --quit, and the tray actions for installing and for start at logon."
  consumes:
  - id: CONS-001
    statement: "InstallSystem: copy and remove files, register the app, set autostart, register native hosts, create launchers, request quit, launch."
  forbidden:
  - id: FORB-001
    statement: "Installing as a service or into a machine-wide location."
  - id: FORB-002
    statement: "File system, registry or process access from the install procedure itself."
variability:
  fixed:
  - id: FIX-001
    statement: "The order: stop the running instance, place files, register, create launchers, start."
  free:
  - id: FREE-001
    statement: "Install locations, registration mechanisms and launcher form per platform."
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: "Each step reports its failure with the path or key involved; the procedure stops and returns it to the user."
trust_boundaries: []
compatibility_policies:
- id: COMPAT-001
  statement: "Running --install over an existing installation is the update path and must leave user data untouched."
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-self-installing-per-user-executable}
source_paths:
- crates/app/src/install/mod.rs
- crates/app/src/platform/win/install.rs
- crates/app/src/platform/win/session.rs
- crates/app/src/platform/mac/install.rs
- crates/app/src/cli.rs
- INSTALL.md
summary: Contract of the self-installing executable, the single resident instance and finalization on every exit path.
---

## Purpose

Let one executable set itself up to run at every logon and be removed cleanly, without administrator rights, and make sure no way of ending the app leaves an unfinalized recording.

## Responsibilities

`install` holds the procedures as shared code over the `InstallSystem` port: `install` (which is also update), `uninstall`, the autostart toggle and the content of the native-host manifest. The platform installers supply the operations. On Windows these are a copy to `%LOCALAPPDATA%\Programs\MeetingAssistant`, a `Run` value, native-host keys under `HKCU`, `.lnk` shortcuts and an uninstall key. On macOS they are a copy to `~/Library/Application Support/MeetingAssistant`, a LaunchAgent plist, native-host JSON files and launcher `.app` bundles.

The platform session code provides the single-instance guard and delivers a quit request to the resident instance. The tray turns a quit request, a menu quit and the end of the user session into the same shutdown path, which stops the engine and with it the recorder.

## Boundaries

The procedure names browsers and paths only through the port. It embeds no platform branch. The extension's files come from `bundle` and are written through `write_file`.

## Invariants

Update safety rests on INV-002: the installer asks the running instance to quit, which finalizes its recording, and proceeds only when that instance has exited. Replacing the executable under a recording would lose the file.

## Collaboration

`cli` parses the commands. `main` runs them before the resident path. The browser launcher receives its switch from `browser::protocol::browser_launch_option`, so the extension ID has a single source.

## Failure Responsibility

The installer prints each completed step and the failing one. It does not roll back completed steps; running `--install` again or `--uninstall` is the recovery, and both are safe to repeat.

## Variability

Locations and mechanisms are per platform. The order of steps and the rule that user data survives uninstall are fixed.

## Conformance

`install/mod.rs` tests the procedures against a fake `InstallSystem` that records calls. Install, update and uninstall were run by hand on Windows; a real logoff was not. The macOS installer has not been run.

## Related Decisions

[ADR: ship one self-installing per-user executable](../adr/adr-20261117-self-installing-per-user-executable.md). The user steps are in `INSTALL.md`.
