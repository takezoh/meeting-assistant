---
id: issue-20261117-macos-has-no-native-window-display-or-ap
kind: issue
title: macOS has no native window, display or app-audio capture
status: open
created: '2026-11-17'
issue_type: plan
tags: []
owners: []
relations: []
subject_paths:
- crates/app/src/platform/mac/mod.rs
summary: On macOS only extension-based tab recording exists; signal collection and the capture backend are not implemented.
---

## Opportunity

On macOS, desktop meeting apps, browser meetings without the extension and manual recording should work as they do on Windows. Today the macOS `PlatformSignalSource` reports no microphone use and no windows, and `PlatformRecorder` returns an error for every target other than a browser tab.

## Evidence

`crates/app/src/platform/mac/mod.rs` implements the installer, the single-instance guard and signal handling only. The platform support tables in `README.md` and `INSTALL.md` list these capabilities as not implemented.

## Context for follow-up

The work is to implement two ports for macOS: `SignalSource` (processes using the microphone, the process tree, windows) and `CaptureBackend` (window and display capture, per-app audio, the file writer), most likely with ScreenCaptureKit and AVFoundation. The shared procedures in `crates/core` and `crates/app/src/recording` already exist and are governed by the [meeting detection](../design/design-meeting-detection.md) and [capture pipeline](../design/design-capture-pipeline.md) designs. The default rules name Windows process names; macOS needs its own. Screen recording and microphone permissions on macOS affect what a background app can capture. The macOS code that does exist should be run on a Mac first; see the related issue on that.
