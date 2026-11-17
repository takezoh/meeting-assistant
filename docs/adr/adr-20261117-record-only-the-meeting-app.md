---
id: adr-20261117-record-only-the-meeting-app
kind: adr
title: Record only the meeting app by default
status: accepted
created: '2026-11-17'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/config.rs
- crates/core/src/coordinator.rs
- crates/app/src/recording/plan.rs
- crates/app/src/recording/pipeline.rs
summary: Default scope is the meeting app only, and a recording is never widened to
  the display or all output audio as a fallback.
updated: '2026-11-17'
---

# Record only the meeting app by default

## Context

The first implementation recorded the whole display and everything played on the output device. The user asked whether video and audio were limited to the meeting app and then directed that recording cover only the target app.

A whole-display recording captures whatever else is on screen: other documents, chat notifications, a password manager. All-output audio captures music and other calls. A person who believes only the meeting is recorded would not check for these.

The same question arises when the intended source is unavailable: the window is not found yet, or the app's audio cannot be opened. Falling back to the display or the output device would produce a fuller file, and would silently record what the user chose not to record.

## Decision

The default `capture_scope` is `app`: video is one window of the meeting app, the other side's audio is the audio of that app's process tree, and the user's audio is the microphone.

In app scope the recording is never widened as a fallback. A missing window gives a black picture until a window appears. App audio that cannot be captured is left out. Whole-display video and all-output audio happen only when the user sets `capture_scope = "screen"`, or starts a manual recording while no meeting is detected, where no target app exists.

## Consequences

- Positive: the file contains what the user expects and nothing they did not choose.
- Negative: a recording can have a black picture or lack the other side's audio where a fallback would have captured something.
- Negative: content the meeting app shows in a separate window, such as a shared screen, is not recorded.
- Neutral: users who prefer completeness over containment can select screen scope.

## Confirmation

Tests in `crates/core/tests/coordinator.rs` assert that a meeting without a window is requested with a blank video target and not the display. Tests in `crates/app/src/recording/plan.rs` and `pipeline.rs` assert that an unavailable audio source is skipped and that a window recording without a picture starts black.


{% transition from="proposed" to="accepted" date="2026-11-17" %}
Decided by the user on 2026-11-17
{% /transition %}
