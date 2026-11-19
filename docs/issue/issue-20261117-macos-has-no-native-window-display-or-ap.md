---
id: issue-20261117-macos-has-no-native-window-display-or-ap
kind: issue
title: macOS has no native window, display or app-audio capture
status: done
created: '2026-11-17'
issue_type: plan
tags: []
owners: []
relations: []
subject_paths:
- crates/app/src/platform/mac/mod.rs
- crates/app/src/platform/mac/recorder/mod.rs
- crates/app/src/platform/mac/signals.rs
summary: macOS signal collection and the ScreenCaptureKit capture backend are implemented;
  capture has not yet run on a Mac.
disposition_target: note-20261119-macos-verification-2026-11-19
updated: '2026-11-19'
---

## Opportunity

On macOS, desktop meeting apps and manual recording should work as they do on Windows. Both are implemented as of 2026-11-18 (see [the macOS capture ADR](../adr/adr-20261118-capture-on-macos-with-screencapturekit-a.md)), but the capture itself has not run on a Mac.

## Evidence

On 2026-11-18 on one Apple Silicon Mac (macOS 26): the signal source listed processes and windows through `--probe`; the AVAssetWriter writer finished a playable two-second MP4 with video and audio; AVAudioEngine delivered microphone sound in the mixer's format. ScreenCaptureKit was not run, because the only process that could run it was the terminal app, which does not have the Screen & System Audio Recording permission and should not be given it for this.

## Context for follow-up

Install the build, give **Meeting Assistant** the Screen & System Audio Recording and Microphone permissions, and record: `--self-test` through `open` (see [testing](../technical/testing.md)), a manual screen recording from the status panel, and a real call in Zoom, Teams or Slack. Check whether the app-audio filter includes sound played by the meeting app's helper processes, and whether the ad-hoc signature makes macOS ask for the permissions again after an update. Record the outcome as a dated note and close this issue against it.


{% transition from="open" to="done" date="2026-11-19" %}
ScreenCaptureKit capture of a display, a meeting window and an app's audio ran on a Mac on 2026-11-19; the permission lost on update is its own issue.
{% /transition %}
