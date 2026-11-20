---
id: adr-20261120-record-the-desktop-or-a-window-by-hand
kind: adr
title: Record the desktop or a chosen window by hand, with or without the microphone
status: proposed
created: '2026-11-20'
decision_makers:
- user
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
- {type: references, target: design-meeting-detection}
- {type: references, target: design-capture-pipeline}
source_paths:
- crates/core/src/target.rs
- crates/core/src/coordinator.rs
- crates/app/src/panel/view.rs
- crates/app/src/panel/panel.html
- crates/app/src/recording/plan.rs
summary: The panel records the entire screen or one window picked from a list it reads,
  a window with its app's sound alone, and can leave the microphone out, which keeps
  its track silent.
---

## Context

The status panel offered one recording by hand besides the detected meetings: the entire screen, the primary display with all output audio and the microphone. The user asked, on 2026-11-20, for the panel's screen recording to choose between the desktop and a window, and for a recording of the screen to choose whether the microphone is included.

Picking a window can be done in the panel, from a list the app reads, or through the system's own picker (`GraphicsCapturePicker` on Windows, `SCContentSharingPicker` on macOS). The system pickers give back a capture item or a content filter rather than a window identifier, which the `Recorder` port and both platforms' window capture take today.

Every recording has two audio tracks in a fixed order, the other side first and the microphone second, and analysis relies on that order ([capture pipeline](../design/design-capture-pipeline.md) INV-011). Recordings are analysed into minutes rather than played.

## Decision

- The panel's Screen section records either the entire screen or one window, chosen in the panel from a list the app reads when the user asks for it: the windows the system shows as windows of their own (on Windows, not cloaked and not tool windows), apart from the app's own, named by their app and title. The user chose the list in the panel over the system pickers.
- A window recorded by hand is recorded with the sound of its app alone, as a meeting's window is in app scope, never with all output audio; the user chose this. The window chosen is not exchanged for another, also when it closes.
- A recording of the desktop or of a window can leave the microphone out. Its track is then written silent and the microphone is not opened, so the file keeps the same tracks in the same order. Meetings are always recorded with the microphone unless the configuration turns it off.
- The choices live in the page while the app runs; they are not settings. The tray menu's "Record entire screen" records the desktop with the microphone, as before.

## Consequences

- Positive: a meeting that is not detected, or anything else on screen, can be recorded as one window without other windows or other apps' sound.
- Positive: the layout of every recording stays the same, so a recording without the microphone is analysed like any other.
- Negative: the list is read when asked, so a window opened later appears only after the list is read again (choosing Window, or opening the panel while Window is chosen).
- Negative: an app whose sound comes from a process outside the window's process tree is recorded without it, as a meeting app's is.

## Confirmation

`crates/core/src/target.rs` tests which windows are offered and how they are named; `crates/core/tests/coordinator.rs` that a window chosen by hand is requested with its app's process and never moved, and that the desktop can leave the microphone out; `recording/plan.rs` that a microphone left out keeps its track without a source; `panel/view.rs` the page's messages and what is shown. The ignored test in `platform/win/signals.rs` checks on Windows that tool and cloaked windows are not offered. A run on one Windows PC is recorded in [a note](../note/note-20261120-window-recording-by-hand-on-windows.md).
