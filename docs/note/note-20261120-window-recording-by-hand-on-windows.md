---
id: note-20261120-window-recording-by-hand-on-windows
kind: note
title: Recording a window by hand checked on Windows
status: draft
created: '2026-11-20'
tags: []
owners: []
relations:
- {type: references, target: adr-20261120-record-the-desktop-or-a-window-by-hand}
- {type: references, target: note-20261119-windows-handoff-after-the-macos-work-of-}
source_paths:
- crates/app/src/panel/panel.html
- crates/app/src/platform/win/signals.rs
summary: On one Windows PC on 2026-11-20 a chosen window and the entire screen were
  recorded from the panel without the microphone, each file with the video and two
  audio tracks of the same length.
---

## Summary

Observed on 2026-11-20 on one PC, Windows 11 (build 26200) with an NVIDIA GeForce RTX 4060 Ti, used over Remote Desktop, with a debug build of the change that lets the panel record the desktop or a chosen window with or without the microphone ([ADR](../adr/adr-20261120-record-the-desktop-or-a-window-by-hand.md)). The panel's page, which the app loads as it starts, was driven through WebView2's DevTools protocol by clicking its own controls, and captured as rendered: over Remote Desktop the panel's window was not seen to show for simulated clicks on the tray icon, with this build and equally with the installed build from before the change. Recordings went to a scratch folder through `--config`. Later the same day the user recorded with the installed app.

## Notes

### The window list

Choosing Window made the page ask for the windows and show them. Thirteen were listed, in the system's order, each by its executable's name and its title: the windows of the apps open on the desktop. Left out were the panel itself, "Command Palette" and "Windows Input Experience" (cloaked) and "Program Manager" (a tool window). The ignored test `only_windows_shown_as_their_own_can_be_picked_to_record` passed and fails when the cloaked check is removed.

### A window recorded without the microphone

A 640x360 window of a Python process, solid green and titled "e2e target", was chosen, the microphone switch turned off and Record pressed. The panel showed "python (e2e target) · Manual · No microphone". The log said `recording is ready: window 1920x1080 30fps audio [other side: app audio (pid 8296), microphone: silence]`.

The file, finished on quitting after 42.7 s, had the video as track 1 and two AAC tracks, 2 and 3. Decoded by Media Foundation, all 1281 pictures had luma 134 in their middle, the luma of that green, so the window was drawn in the middle of the canvas; track 3, the microphone's, peaked at 0, silent throughout; track 2, the app's sound, peaked at 2 of 32767, as the window's app played nothing.

### Recorded by the user with the installed app

The build of b011ada was installed on the same PC. The user, connected to it, opened the panel from the tray icon, recorded a window and then the entire screen, and reported having checked both. The user had turned the microphone switch off for both, and the log named no source for the microphone track in either, with no warning.

- A window of Orca, for 22.9 s: `recording is ready: window 1920x1080 30fps audio [other side: app audio (pid 6300), microphone: silence]`. The app's sound peaked at 0.0%.
- The entire screen, for 3.2 s: `recording is ready: display 1920x1080 30fps audio [other side: output device audio, microphone: silence]`. Display capture delivered 39 pictures; the output device delivered no sound, nothing being played.

Each file had the video as track 1 and two AAC tracks, 2 and 3, all three of the same length (22.87 s, and 3.17 to 3.18 s, summed over the fragments), and carried the app `Manual` and its start as metadata. Their pictures and sound were not decoded in this session.

### Not checked

- Sound: no recorded app played any, and the microphone was left out of every recording, so neither an app's sound nor a voice on the microphone track has been heard in a recording made this way.
- A window that closes during the recording.
- macOS: nothing of this change has run on a Mac, and the macOS target could not be type-checked from this PC.
