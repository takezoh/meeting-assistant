---
id: note-20261119-windows-handoff-after-the-macos-work-of-
kind: note
title: Windows handoff after the macOS work of 2026-11-19
status: draft
created: '2026-11-19'
topic: handoff
summary: What changed on 2026-11-18 and 2026-11-19 that has only been type-checked for Windows, what is expected to
  fail there, and the order in which to check it on a Windows machine.
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-performance-handoff}
- {type: references, target: issue-20261119-tab-recordings-on-windows-will-keep-the-}
- {type: references, target: design-capture-pipeline}
- {type: references, target: design-browser-tab-recording}
- {type: references, target: design-meeting-detection}
source_paths:
- crates/app/src/platform/win/recorder/writer.rs
- crates/app/src/platform/win/recorder/mod.rs
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/peers.rs
- crates/app/src/recording/mp4.rs
- crates/app/src/recording/merge.rs
- crates/app/src/linked.rs
- crates/core/src/detector.rs
- extension/hook.js
---

## Summary

All work of 2026-11-18 and 2026-11-19 was done and run on one Mac. For Windows the code was only type-checked and linted from the Mac (`cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc`); none of it has run on Windows. This note lists what a session on a Windows machine has to check, what is expected to fail, and where to start. It does not settle any design; the contracts are in the designs it references.

The Windows performance work found by reading the source is a separate handoff: [Windows recording performance](note-20261118-windows-recording-performance-handoff.md). Part of it is now settled; see "Interaction with the performance handoff" below.

## Notes

### Goal

The Windows build records what the macOS build was shown to record on 2026-11-19: two audio tracks (the other side first, the microphone second) in desktop, screen and browser tab recordings, the call window rather than the app's main window, recordings that end a few seconds after the meeting, and a picture of `video.width` by `video.height` (1920x1080 by default) whatever the window does.

### What changed and has not run on Windows

| Change | Commit | Windows code involved | Shared code (tested on the Mac) |
| --- | --- | --- | --- |
| Two audio tracks in every recording; a recording of the microphone alone | 98267e2 | `writer.rs`: one AAC stream per track, video optional; `mod.rs`: `write_audio` with a track index, no scaler without video | `recording/{plan,pipeline,session}.rs` |
| The microphone recorded by the app beside a browser tab, added as the second audio track | 98267e2 | the microphone-only recording through WASAPI capture and the sink writer | `linked.rs`, `recording/{merge,mp4}.rs`, `adapters` (`Mp4Remuxer`, `DiskFiles`) |
| The upload hello sent after `MediaRecorder.start()` as the tab's time zero; the warning message removed | 98267e2 | none | `extension/offscreen.js`, `browser/{protocol,server}.rs` |
| The window that appeared with a meeting is recorded and keeps it going; desktop end grace 2 s, browser 1 s; a reconnecting call counts as connected | ee9ea9c | window enumeration as it was | `crates/core/src/{detector,engine,rules}.rs`, `extension/hook.js` |
| Every picture is fitted into `video.width` by `video.height` (1080p by default and at most) | ee9ea9c | `open_video` no longer reports a size; the scaler fits every frame into the configured canvas | `recording/pipeline.rs`, `crates/core/src/config.rs` |
| Loopback connections admitted only from the current user's processes | d17cd72 | `peers.rs` (TCP table and process token) | `browser/peer.rs` |

### Expected to fail

- **The microphone will not be added to tab recordings.** The Windows writer produces fragmented MP4, and `recording::mp4` reads a source's samples from its `moov` tables only. See [the issue](../issue/issue-20261119-tab-recordings-on-windows-will-keep-the-.md). Fix this first; it can be developed and unit-tested on any OS.

### Open questions only Windows can answer

- Whether the Media Foundation sink writer accepts two AAC streams, and a file with no video stream, in fragmented MP4.
- Whether Teams, Zoom and Webex on Windows open a separate call window as the call starts (the meeting window), whether it closes when the call ends, and whether they release the microphone while muted. If a call window stays open after the call, the recording does not end until it is closed.
- Whether the D3D11 video processor scales a window that changes shape mid-recording cleanly into the fixed canvas.
- Whether the Windows microphone privacy setting for desktop apps silences the microphone track as it does on macOS.

### Order of work on a Windows machine

1. `cargo test --workspace` on Windows, including `cargo test -p meeting-assistant peers`, which has never run.
2. Fix the issue above, with a fragmented microphone recording added to the tests in `recording/mp4.rs`.
3. `meeting-assistant --self-test <path>`, then `ffprobe` the file: one H.264 video at 1920x1080 and two AAC tracks. Play sound and speak during it; the sound should be only in the first audio track and the voice only in the second (a left-only and a right-only test tone tells a digital capture from the microphone picking up the speakers).
4. A tab recording in Chrome or Edge started from the launcher: two audio tracks, the user's voice in the second, no `.microphone.m4a` left beside it, and the recording ending about 3 seconds after leaving.
5. A desktop call in Teams or Zoom: which window is recorded, whether the recording continues while muted, and how soon it ends after leaving. `meeting-assistant --probe` lists the apps' windows with their titles and which processes use the microphone and play audio; run it in each state.
6. Record the outcome as a dated note, and update `PRODUCT.md` "What is not established".

### Interaction with the performance handoff

As of commit ee9ea9c the canvas is fixed at the configured size (1920x1080 by default), so in [the performance handoff](note-20261118-windows-recording-performance-handoff.md) F7, F8, I5, step 3 and step 7 no longer apply, and the per-frame allocations of F6 now happen for almost every frame, since a window rarely has exactly the canvas's size. Each recording now also has two audio sources primed separately in the mixer, which bears on I1.

### Agreed direction and limits

- No compatibility with older app or extension versions, and no fallbacks for them.
- Video is for context only; recordings are analysed into minutes. Nothing above 1080p, and smaller windows may be scaled up (the user chose to keep the system's fitting rather than avoid scaling up).
- Keep the boundaries in AGENTS.md: deciding code in `recording/` and `crates/core`, OS calls under `platform/win`.
- `--install` changes the user's registry, Start Menu and Desktop; run it only when asked, and stop a running copy with `--quit` first.

### Reading list

[AGENTS.md](../../AGENTS.md), [the capture pipeline design](../design/design-capture-pipeline.md), [the browser tab recording design](../design/design-browser-tab-recording.md), [the meeting detection design](../design/design-meeting-detection.md), and the verification notes of 2026-11-19: [macOS capture](note-20261119-macos-verification-2026-11-19.md), [two audio tracks](note-20261119-two-audio-tracks-verification-2026-11-19.md), [meeting window and end detection](note-20261119-meeting-window-and-end-detection-verific.md).
