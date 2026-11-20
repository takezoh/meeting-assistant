---
id: note-20261120-window-recording-by-hand-on-a-mac
kind: note
title: Recording a window by hand checked on a Mac
status: draft
created: '2026-11-20'
tags: []
owners: []
relations:
- {type: references, target: adr-20261120-record-the-desktop-or-a-window-by-hand}
- {type: references, target: note-20261120-window-recording-by-hand-on-windows}
- {type: references, target: note-20261119-windows-handoff-after-the-macos-work-of-}
source_paths:
- crates/app/src/platform/mac/signals.rs
- crates/app/src/platform/mac/recorder/mod.rs
- crates/app/src/panel/panel.html
summary: On one Mac on 2026-11-20 windows and the entire screen were recorded from
  the panel with and without the microphone; a window took its app's sound only, and
  macOS showed no microphone indicator.
---

## Summary

Observed on 2026-11-20 on one Apple Silicon Mac with macOS 26, with the release build of e3ad51c, the change that lets the panel record the desktop or a chosen window with or without the microphone ([ADR](../adr/adr-20261120-record-the-desktop-or-a-window-by-hand.md)), installed with `--install`. Before installing, `cargo fmt --check`, clippy for macOS (all targets) and for `x86_64-pc-windows-msvc`, `cargo test --workspace` and the extension's tests passed on that Mac. After the update the Screen & System Audio Recording permission was reset and granted again; a self-test then wrote 5.07 s of 1920x1080 video and two AAC tracks.

The user opened the panel from the menu bar icon and recorded a window and then the entire screen, both with the microphone switch off, and later five more windows of Chrome and Finder with and without the microphone. The [Windows note](note-20261120-window-recording-by-hand-on-windows.md) has the same checks on Windows.

## Notes

### The window list

Choosing Window showed a list of windows, as the user reported; its entries were not recorded in this session. The user chose a window of Google Chrome from it.

### A window recorded without the microphone

The Chrome window was playing a YouTube video. The log said `recording is ready: window 1920x1080 30fps audio [other side: app audio (pid 90234), microphone: silence]`; pid 90234 was `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, Chrome's main process, the only one named to the capture. The sound capture ended with 515 buffers, the loudest at -6.6 dBFS.

The file, 10.23 s long, had the video and two AAC tracks of the same length and carried `com.meeting-assistant.app` `Manual` and `com.meeting-assistant.started_at` `2026-11-20T14:46:33+09:00`. Measured with ffmpeg's `volumedetect`, the first audio track had a mean of -17.8 dB and a peak of -6.3 dB; the second, the microphone's, was at -91 dB throughout, which is digital silence. A picture 5 s in held the Chrome window alone; the window being narrower than 16:9, ScreenCaptureKit had put it at the left of the canvas with black to its right, where the Windows build puts a window in the middle.

### The entire screen without the microphone

The log said `recording is ready: display 1920x1080 30fps audio [other side: output device audio, microphone: silence]`. The file, 6.93 s long, had the video and two AAC tracks of the same length and the same two metadata keys. The first audio track had a mean of -17.2 dB and a peak of -6.8 dB; the second was at -91 dB throughout. A picture 3 s in showed the panel reading "Entire screen · Manual · No microphone", with the microphone switch off and "Left out; its track is silent" beside it.

### Windows of Chrome and Finder, with and without the microphone

Between 15:02 and 15:05 the user recorded five more windows. The log named Chrome's main process (pid 90234) or Finder (pid 71009) as the source of the first audio track, and the microphone or silence for the second, as the switch was set. The two tracks measured as follows (mean and peak; -91 dB is digital silence):

| Start | Window | Microphone | First track | Second track |
| --- | --- | --- | --- | --- |
| 15:02:39 | Chrome | on | -91 / -91 dB | -58.2 / -44.6 dB |
| 15:02:58 | Chrome | off | -20.3 / -7.1 dB | -91 / -91 dB |
| 15:03:55 | Finder | off | -91 / -91 dB | -91 / -91 dB |
| 15:04:31 | Finder | on | -91 / -91 dB | -44.9 / -24.1 dB |
| 15:04:52 | Finder | on | -91 / -91 dB | -57.7 / -33.6 dB |

The user reported what was played and heard: the microphone's sound was recorded when the switch was on and not when it was off; a Chrome window's recording took the sound that another Chrome window played; and a window's recording did not take another app's sound. The Finder recordings' first tracks held nothing. This matches the capture being of an app, not a window: ScreenCaptureKit is given the app of the window's process.

### The microphone indicator

The user reported that macOS showed no microphone indicator in the menu bar in any of these recordings, also while the microphone was recorded. The indicator therefore tells nothing on this Mac about whether the app opened the microphone; the log's `microphone: silence` and the silent track do. Why macOS showed none is not known.

The log also has `Looking at the windows` within 0.5 s of the start of each of the five later recordings, with the microphone on or off, and `Stopped looking at the windows.` 1 to 24 s after each ended; for the first two recordings it ran from 14:46:19 to 14:47:13, and once it began 10 s before a recording. With no meeting recorded, the engine reads the windows only while Core Audio reports some process running audio input. Which process that was is not logged; `--probe` at about 14:55, with nothing recording, listed none.

### Not checked

- The entire screen recorded from the panel with the microphone on. The entire screen with the microphone was recorded from the tray menu on 2026-11-19.
- A window that closes during the recording. Minimized windows and windows on another Space are not listed, as the window list holds only windows on screen.
- A tab recording with the microphone added using the `recording::mp4` of d1a8127, which the [Windows handoff](note-20261119-windows-handoff-after-the-macos-work-of-.md) also asks a Mac to run.
