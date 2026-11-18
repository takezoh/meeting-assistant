---
id: issue-20261117-macos-code-has-never-run-on-a-mac
kind: issue
title: macOS code has never run on a Mac
status: done
created: '2026-11-17'
issue_type: plan
tags: []
owners: []
relations: []
subject_paths:
- crates/app/src/platform/mac/mod.rs
- crates/app/src/platform/mac/install.rs
summary: The macOS installer, launcher, native-host registration and signal handling
  are type-checked on Windows only and need a first run on a Mac.
disposition_target: note-20261118-macos-verification-2026-11-18
updated: '2026-11-18'
---

## Opportunity

The macOS installation and the extension-based tab recording should be verified on a Mac. Until then every macOS statement in the documentation describes intended behavior.

## Evidence

The macOS target is checked only with `cargo clippy --workspace --target aarch64-apple-darwin` from Windows. No binary for macOS has been linked or run. `README.md`, `INSTALL.md` and `PRODUCT.md` state this.

## Context for follow-up

Things that a type check cannot show and that need observing: whether the executable links; the LaunchAgent plist loads and starts the app at login; the native-messaging host JSON is found by Chrome and Edge; the launcher `.app` starts the browser with `--allowlisted-extension-id`; the tray icon appears; SIGTERM finalizes a recording; and a tab recording arrives and is stored under `~/Movies/MeetingAssistant`. The [installation design](../design/design-installation.md) and the [browser tab recording design](../design/design-browser-tab-recording.md) hold the contracts. Record the outcome as a dated note.
