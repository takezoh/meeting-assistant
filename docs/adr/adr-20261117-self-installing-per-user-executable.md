---
id: adr-20261117-self-installing-per-user-executable
kind: adr
title: Ship one self-installing per-user executable
status: accepted
created: '2026-11-17'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/app/src/install/mod.rs
- crates/app/src/platform/win/install.rs
- crates/app/src/platform/mac/install.rs
summary: The executable installs itself per user without elevation and runs in the
  user session, never as a service.
updated: '2026-11-17'
---

# Ship one self-installing per-user executable

## Context

The app has to run for every logon session, be registered with browsers as a native-messaging host, and provide a browser launcher with a command-line switch. The user asked how it is installed, whether it is a service, and when it starts.

A Windows service runs in session 0 without access to the user's desktop, windows or audio session, so it cannot capture. A separate installer package (MSI, MSIX) adds a build toolchain and, for machine-wide installs, elevation. Everything the app needs to register exists per user.

## Decision

The executable installs itself with `--install`: it copies itself into the user's profile, registers start at logon, the native-messaging host and an uninstall entry, writes out the embedded extension, creates browser launchers and starts the installed copy. `--uninstall` reverses it and keeps user data. Running `--install` again is the update path. The app runs as an ordinary process in the user's session, never as a service.

The install procedure is shared code over a port; each platform supplies the operations.

## Consequences

- Positive: no administrator rights, no installer toolchain, one file to distribute.
- Positive: install, update and uninstall are testable against a fake.
- Negative: the app is installed per user; other users of the machine must install it themselves.
- Negative: there is no code signing or packaging, so Windows may warn about an unknown publisher, and notifications are not attributed to the app.
- Neutral: the app does not run while nobody is logged on, when nothing could be captured anyway.

## Confirmation

Tests in `crates/app/src/install/mod.rs` assert the order of steps, that nothing changes when the running instance cannot be stopped, and that uninstall unregisters before deleting. Install, update and uninstall were run by hand on Windows on 2026-11-17.


{% transition from="proposed" to="accepted" date="2026-11-17" %}
Decided by the user on 2026-11-17
{% /transition %}
