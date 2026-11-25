---
id: design-installation
kind: design
title: Per-user installation and lifecycle
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: 'Install, update and uninstall the app for the current user: files, start
    at logon, native-host registration, browser launchers, uninstall entry.'
- id: RESP-004
  statement: 'Make the file a user installs from, signed by the build: on macOS a disk
    image holding the signed app bundle, on Windows the executable with its version
    information, icon and manifest, signed when a certificate is named. Opened from
    elsewhere, that app offers to install itself.'
- id: RESP-002
  statement: Ensure a single resident instance and let another invocation ask it to
    finalize and exit.
- id: RESP-003
  statement: Finalize a recording before the process ends on quit, logoff, shutdown
    or a termination signal.
invariants:
- id: INV-001
  statement: Installation changes only the current user's profile and needs no elevation.
  enforcement: review
- id: INV-002
  statement: If the running instance cannot be made to exit, install and uninstall
    change nothing.
  enforcement: test
- id: INV-003
  statement: An installation that fails part-way stops at the failure and does not
    start the app.
  enforcement: test
- id: INV-004
  statement: Uninstall removes every registration before removing files and keeps
    configuration, logs and recordings.
  enforcement: test
- id: INV-005
  statement: The native-host manifest names the installed executable and only this
    extension's origin.
  enforcement: test
- id: INV-006
  statement: Native hosts and launchers are created only for browsers that are present.
  enforcement: test
- id: INV-007
  statement: The installer places the app as it was built. It refuses, before changing
    anything, an app whose signature does not verify where the OS ties permissions to
    it, and it neither writes into nor signs the app it places.
  enforcement: test
- id: INV-008
  statement: A built app opened without a command from somewhere other than the install
    location asks before it installs itself, and declining changes nothing. A run with
    --config never asks.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: The commands --install, --uninstall and --quit, the question a built
      app asks when opened from elsewhere, and the tray actions for installing and for
      start at logon.
  - id: PROV-002
    statement: packaging/macos/build.sh, which builds, signs and puts the macOS app
      into a disk image; packaging/windows/build.ps1, which builds the Windows executable
      and signs it when a certificate is named; and crates/app/build.rs, which gives the
      Windows executable its resources.
  consumes:
  - id: CONS-001
    statement: 'InstallSystem: copy the app and remove files, register the app, set
      autostart, register native hosts, create launchers, request quit, launch, say whether
      a built app installs itself when opened, read how the app was signed and the installed
      app''s identity, clear the permissions kept for an earlier identity.'
  forbidden:
  - id: FORB-001
    statement: Installing as a service or into a machine-wide location.
  - id: FORB-002
    statement: File system, registry or process access from the install procedure
      itself.
  - id: FORB-003
    statement: Signing the app on the user's computer, or changing what was signed.
variability:
  fixed:
  - id: FIX-001
    statement: 'The order: stop the running instance, place files, register, create
      launchers, start.'
  free:
  - id: FREE-001
    statement: Install locations, registration mechanisms and launcher form per platform.
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: Each step reports its failure with the path or key involved; the procedure
    stops and returns it to the user.
trust_boundaries: []
compatibility_policies:
- id: COMPAT-001
  statement: Running --install over an existing installation is the update path and
    must leave user data untouched.
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-self-installing-per-user-executable}
- {type: references, target: adr-20261124-distribute-a-built-app-that-installs-itself}
source_paths:
- crates/app/src/install/mod.rs
- crates/app/src/platform/win/install.rs
- crates/app/src/platform/win/session.rs
- crates/app/src/platform/mac/install.rs
- crates/app/src/cli.rs
- crates/app/src/main.rs
- packaging/macos/build.sh
- packaging/macos/Info.plist
- packaging/macos/entitlements.plist
- packaging/windows/build.ps1
- crates/app/build.rs
- crates/app/windows/resources.rs
- crates/app/windows/app.manifest
- INSTALL.md
summary: Contract of the self-installing app, the file it is distributed in, the single
  resident instance and finalization on every exit path.
---

## Purpose

Let a user install the app from one file, without administrator rights or development tools: set it up to run at every logon and remove it cleanly. Make sure no way of ending the app leaves an unfinalized recording.

## Responsibilities

`install` holds the procedures as shared code over the `InstallSystem` port:

- `install`, which is also the update
- `uninstall`
- the autostart toggle
- the content of the native-host manifest
- whether an opened app offers to install itself, and the question it asks

The platform installers supply the operations:

| | Windows | macOS |
| --- | --- | --- |
| The app | The executable, copied to `%LOCALAPPDATA%\Programs\MeetingAssistant\meeting-assistant.exe` without the mark the browser gave the download | `~/Applications/Meeting Assistant.app`, copied whole with `ditto` as the build signed it; its own `Info.plist` lists it |
| Start at logon | A `Run` value | A LaunchAgent plist |
| Native hosts | Keys under `HKCU` | JSON files |
| Browser launchers | `.lnk` shortcuts | Launcher `.app` bundles |
| Other | An uninstall key | The extension in `~/Library/Application Support/MeetingAssistant`; the installed app is started through LaunchServices |

The file a user installs from is made by the build ([the decision](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md)). On macOS, `packaging/macos/build.sh` does this:

1. Builds the release executable.
2. Puts it into a bundle with `packaging/macos/Info.plist`.
3. Signs the whole bundle as `com.meeting-assistant.app`, with the hardened runtime and the entitlements in `packaging/macos/entitlements.plist`. By default it signs with the identity `Meeting Assistant Local Signing`; `--identity` names another, such as a Developer ID.
4. Puts the bundle into a disk image, which `--notary-profile` also notarizes.

The installer checks that the bundle's signature verifies before it changes anything, and reports who signed it. It clears the app's screen recording permission when the installed app's designated requirement changed across the update ([the earlier decision](../adr/adr-20261124-sign-the-macos-app-with-a-lasting-identity.md)). On macOS a built app is one that runs from a bundle.

On Windows, `packaging/windows/build.ps1` builds the release executable, copies it to `target\windows\Meeting-Assistant-<version>.exe`, and signs it with `signtool` when a certificate is named. `crates/app/build.rs` gives every build of the executable its version information, icon and application manifest, so a Windows executable is always a built app: opened from elsewhere, also when `cargo build` made it, it asks. Windows ties no permission to a signature, so the installer reads none. It removes the `Zone.Identifier` stream from the copy it places, because the copy keeps the stream of the downloaded file, and Windows would check the installed app as a download again each time the shell starts it.

The platform session code provides the single-instance guard and delivers a quit request to the resident instance. The tray turns a quit request, a menu quit and the end of the user session into the same shutdown path, which stops the engine and with it the recorder. On macOS the end of the session reaches the app as a request to terminate the application, which ends the tray's event loop; the tray finalizes on that loop's last event as well, once.

## Boundaries

The procedure names browsers and paths only through the port. It embeds no platform branch. The extension's files come from `bundle` and are written through `write_file`.

## Invariants

Update safety rests on INV-002: the installer asks the running instance to quit, which finalizes its recording, and proceeds only when that instance has exited. Replacing the executable under a recording would lose the file.

Permission continuity on macOS rests on INV-007: macOS keeps the permissions of an app whose designated requirement stays the same. That requirement comes from the signature the build made, which the installer neither replaces nor breaks by writing into the bundle.

## Collaboration

`cli` parses the commands. `main` runs them before the resident path. With no command and no `--config`, `main` asks `install::offers_installation` first. When it is true, `main` puts the question through `platform::confirm`, runs `install` if it is accepted, and exits either way. The browser launcher receives its switch from `browser::protocol::browser_launch_option`, so the extension ID has a single source.

## Failure Responsibility

The installer prints each completed step and the failing one. It does not roll back completed steps; running `--install` again or `--uninstall` is the recovery, and both are safe to repeat.

## Variability

Locations, mechanisms and the form of the distributed file are per platform. So is who signs it, and with which identity. The order of steps and the rule that user data survives uninstall are fixed.

## Conformance

`install/mod.rs` tests the procedures against a fake `InstallSystem` that records calls. The tests cover these points:

- An app whose signature does not verify changes nothing.
- The app is placed as built.
- A changed identity clears the earlier permissions before anything runs the app.
- Only a built app that is not installed offers to install itself.

`platform/mac/install.rs` checks that `packaging/macos/Info.plist` names the identifier and executable the installer uses. `platform/win/install.rs` reads the version information, the icon and the manifest from the test executable, which `build.rs` gives them as it gives the app, and checks that the version information names the file the installer places.

What was run by hand:

- Install, update and uninstall on Windows on 2026-11-17, and on macOS on 2026-11-18, on macOS during a recording. A real logoff was not performed on either.
- On one Mac on 2026-11-24: signing with the identity, the one-time clearing of the screen recording permission, and an update that kept both permissions. That was with the installer that signed the app itself. See the [macOS verification note](../note/note-20261118-macos-verification-2026-11-18.md).
- On the same Mac on 2026-11-24, the build script made the disk image. Its app had the hardened runtime, the microphone entitlement, and the designated requirement of the app installed there before.
- On 2026-11-25 that disk image was installed, declined once, and installed again over itself. The installed app recorded the screen and the microphone without a permission prompt. See [the note](../note/note-20261125-installing-from-the-macos-disk-image-2026-11-25.md). A downloaded disk image, and so Gatekeeper, was not run.
- On one Windows PC on 2026-11-25, an executable built by `cargo build --release` asked when opened, and changed nothing when declined. A copy of it marked as downloaded was opened while the installed app recorded, and accepted: the running app finished its recording and exited, and the copy placed itself without the mark and started. See [the note](../note/note-20261125-installing-from-the-windows-executable-2.md). The build script made the unsigned executable. SmartScreen and signing were not run.

## Related Decisions

- [ADR: ship one self-installing per-user executable](../adr/adr-20261117-self-installing-per-user-executable.md)
- [ADR: distribute a built and signed app that installs itself](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md)

The user steps are in `INSTALL.md`. Building the disk image and the Windows executable is in [the packaging reference](../technical/packaging.md).
