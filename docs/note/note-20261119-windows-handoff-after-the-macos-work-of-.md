---
id: note-20261119-windows-handoff-after-the-macos-work-of-
kind: note
title: Windows handoff after the macOS work of 2026-11-19
status: draft
created: '2026-11-19'
topic: handoff
summary: What changed on 2026-11-18 and 2026-11-19, up to 6900d1b, that has only been
  type-checked for Windows, what a Windows session has run of it, and what remains
  to run on Windows and, for the Windows session's own change, on a Mac.
tags: []
owners: []
relations:
- {type: references, target: issue-20261119-tab-recordings-on-windows-will-keep-the-}
- {type: references, target: design-capture-pipeline}
- {type: references, target: design-browser-tab-recording}
- {type: references, target: design-meeting-detection}
- {type: references, target: note-20261119-windows-encoder-and-1080p}
- {type: references, target: note-20261119-windows-idle-and-recording-resources}
- {type: references, target: adr-20261119-tell-meeting-windows-apart-through-app-adapters}
- {type: references, target: adr-20261119-end-a-desktop-meeting-on-the-microphone}
- {type: references, target: note-20261119-slack-huddle-window-after-the-merge}
- {type: references, target: note-20261119-macos-idle-resources-after-the-merge}
- {type: references, target: note-20261119-windows-checks-after-the-macos-merge}
- {type: references, target: adr-20261120-record-the-desktop-or-a-window-by-hand}
- {type: references, target: note-20261120-window-recording-by-hand-on-windows}
- {type: references, target: note-20261120-window-recording-by-hand-on-a-mac}
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
- crates/core/src/apps.rs
- crates/app/src/apps
- crates/core/src/engine.rs
- crates/app/src/recording/fit.rs
---

## Summary

All macOS work of 2026-11-18 and 2026-11-19 was done and run on one Mac, up to commit 6900d1b. For Windows the code was only type-checked and linted from the Mac (`cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc`); none of it has run on Windows. This note lists what a session on a Windows machine has to check, what is expected to fail, and where to start. It does not settle any design; the contracts are in the designs and ADRs it references.

A Windows session worked on its own at the same time: NV12 encoding up to 1080p on any GPU, fewer signals read while idle, recorded sound started with the picture, and a versioned configuration file. Its results are in [Windows encoder and 1080p](note-20261119-windows-encoder-and-1080p.md) and [Windows idle and recording resources](note-20261119-windows-idle-and-recording-resources.md). Both lines of work were merged on the Mac in d0a8ed3; "After the merge" below says what the merge and the two commits after it changed on the Windows side, none of which has run on Windows.

## Notes

### Done on Windows, 2026-11-19

Recorded in [Windows checks after the macOS merge](note-20261119-windows-checks-after-the-macos-merge.md) and committed in d1a8127: step 1, with the extension's tests of 6900d1b; step 2, with the issue below fixed; step 3 without a microphone, since the PC had no default one; and step 7. The sink writer accepts two AAC streams and a file without video in fragmented MP4, and a window smaller than the canvas is drawn in its middle at its own size. Steps 3 with a microphone, 4, 5 and 6 remain on Windows, and the fix of step 2 has not run on a Mac (see "For a Mac" below).

### Goal

The Windows build records what the macOS build was shown to record on 2026-11-19: two audio tracks (the other side first, the microphone second) in desktop, screen and browser tab recordings; the meeting's own window, never the app's main window, black until that window is found; recordings that end about 3 seconds after the app lets go of the microphone; and a picture of `video.width` by `video.height` (1920x1080 by default) whatever the window does.

### What changed and has not run on Windows

| Change | Commit | Windows code involved | Shared code (tested on the Mac) |
| --- | --- | --- | --- |
| Two audio tracks in every recording; a recording of the microphone alone | 98267e2 | `writer.rs`: one AAC stream per track, video optional; `mod.rs`: `write_audio` with a track index, no scaler without video | `recording/{plan,pipeline,session}.rs` |
| The microphone recorded by the app beside a browser tab, added as the second audio track | 98267e2 | the microphone-only recording through WASAPI capture and the sink writer | `linked.rs`, `recording/{merge,mp4}.rs`, `adapters` (`Mp4Remuxer`, `DiskFiles`) |
| The upload hello sent after `MediaRecorder.start()` as the tab's time zero; the warning message removed | 98267e2 | none | `extension/offscreen.js`, `browser/{protocol,server}.rs` |
| The window that appeared with a meeting is recorded; desktop end grace 2 s, browser 1 s; a reconnecting call counts as connected | ee9ea9c | window enumeration as it was | `crates/core/src/{detector,engine,rules}.rs`, `extension/hook.js` |
| Every picture is fitted into `video.width` by `video.height` (1080p by default and at most); a smaller one is not scaled up | ee9ea9c, d0a8ed3 | `open_video` is given the configured canvas; the video processor places each picture where `recording/fit` says | `recording/{pipeline,fit}.rs`, `crates/core/src/config.rs` |
| Loopback connections admitted only from the current user's processes | d17cd72 | `peers.rs` (TCP table and process token) | `browser/peer.rs` |
| No fallback to the app's largest window: without an adapter's word, a meeting's window must appear after the windows began to be read, which happens only while some process uses the microphone or an app's meeting is recorded (the Windows session's conditions) | d0a8ed3 | `signals.rs` as the Windows session left it | `crates/core/src/{detector,engine}.rs` |
| Meeting app adapters: a port through which an adapter tells an app's main window from a meeting's; Slack's reads the "[Main]" mark in its main window's title | cbbff56 | none | `crates/core/src/apps.rs`, `crates/app/src/apps`, `crates/core/src/{rules,detector}.rs` |
| A desktop meeting continues and ends on its app's microphone use alone; its window decides the picture only | ca9bd37 | none | `crates/core/src/{detector,engine}.rs` |
| Every finished recording carries the meeting's metadata as QuickTime keys, written by rewriting the file once | a3aacab | none | `recording/{mp4,annotate}.rs`, `linked.rs`, `crates/core/src/metadata.rs` |
| A tab capture is asked for only once the recording page is ready, and a start that fails releases it | 6900d1b | none | `extension/{background,offscreen}.js`, with tests in `extension/test` |

### Expected to fail

- **The microphone will not be added to tab recordings.** The Windows writer produces fragmented MP4, and `recording::mp4` reads a source's samples from its `moov` tables only. See [the issue](../issue/issue-20261119-tab-recordings-on-windows-will-keep-the-.md). Fixed on 2026-11-19.

### Open questions only Windows can answer

- Whether the Media Foundation sink writer accepts two AAC streams, and a file with no video stream, in fragmented MP4. **Answered on 2026-11-19:** it does.
- Whether Slack on Windows puts "[Main]" in its main window's title while a joined huddle is open, as on macOS. If it does not, a huddle is recorded black throughout: Slack opens the huddle window as it takes the microphone, so when the window appeared cannot tell it from the main window.
- Whether Teams, Zoom and Webex open a separate call window, and whether it opens after the app takes the microphone. A window already open when the microphone is taken is not recorded unless the app's adapter says it is a meeting's; an app without a separate call window is recorded black.
- Whether Teams, Zoom and Webex let go of the microphone while muted. If one does, a mute longer than the end grace ends the recording; keeping it going would belong in that app's adapter ([ADR](../adr/adr-20261119-end-a-desktop-meeting-on-the-microphone.md)).
- Whether the D3D11 video processor puts a window smaller than the canvas in its middle at its own size. **Answered on 2026-11-19:** it does; the ignored test `a_picture_smaller_than_the_canvas_is_drawn_in_its_middle_at_its_own_size` checks it.
- Whether the Windows microphone privacy setting for desktop apps silences the microphone track as it does on macOS.

### Order of work on a Windows machine

1. `cargo test --workspace` on Windows, and the ignored recorder tests (`cargo test -p meeting-assistant -- --ignored --nocapture`), which ran on Windows before the merge but not with it.
2. Fix the issue above, with a fragmented microphone recording added to the tests in `recording/mp4.rs`.
3. `meeting-assistant --self-test <path>`, then `ffprobe` the file: one H.264 video at 1920x1080 and two AAC tracks. Play sound and speak during it; the sound should be only in the first audio track and the voice only in the second (a left-only and a right-only test tone tells a digital capture from the microphone picking up the speakers).
4. A tab recording in Chrome or Edge started from the launcher: two audio tracks, the user's voice in the second, no `.microphone.m4a` left beside it, and the recording ending about 3 seconds after leaving.
5. A Slack huddle: preview, join, mute for a few seconds, leave. Run `meeting-assistant --probe` in each state; it prints each window of a meeting app with its title and what the app's adapter tells of it (`Main`, `Meeting` or `Unknown`). The recording should start black, show the huddle window from joining, never Slack's main window, go on through the mute, and end about 3 seconds after leaving. The log names the window the recording is asked to move to. On the Mac the huddles were also watched every 200 ms for the app's windows and the processes taking microphone input ([the huddle note](note-20261119-slack-huddle-window-after-the-merge.md)); doing the same with `EnumWindows` and the capture sessions tells the order of window and microphone.
6. A desktop call in Teams or Zoom, the same way. If its call window cannot be told from its main window, write an adapter for it in `crates/app/src/apps` with tests on the titles observed, and record them in a note.
7. Measure the idle CPU again as in [the idle note](note-20261119-windows-idle-and-recording-resources.md), with the merged build; the signals are read on the Windows session's conditions, but that has not been measured on Windows since the merge.
8. Record the outcome as a dated note, and update `PRODUCT.md` "What is not established".

### For a Mac

d1a8127 changed shared code on Windows that has not run on a Mac, and the macOS target could not be type-checked from Windows.

- `recording::mp4` reads a source's samples from its fragments as well as from its sample tables; a fragment without a `tfdt` starts where the track's previous fragment ended. Adding a track now moves explicit `tfhd` base offsets and the `mfra` with the grown `moov`, as writing metadata already did, and both go through the same code.
- On a Mac: `cargo clippy --workspace --all-targets` and `cargo test --workspace`; then a desktop recording, whose metadata is written into AVAssetWriter's output, with every sample unchanged; and a tab recording with the microphone added as its last track and heard where it was spoken.
- Run on one Mac on 2026-11-20 with this code: clippy for macOS, `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`, the whole test suite and the extension's tests passed. Metadata written into two AVAssetWriter files (a self-test and a Slack huddle recording) left every packet's timing and data unchanged, and ffprobe and AVFoundation read the four keys. A tab recording with the microphone added has not been made with it.

On 2026-11-20 the Windows session also let the status panel record the entire screen or a window chosen from a list, with or without the microphone ([ADR](../adr/adr-20261120-record-the-desktop-or-a-window-by-hand.md)); on Windows a window and the entire screen were recorded with the microphone left out ([note](note-20261120-window-recording-by-hand-on-windows.md)). The macOS code changed only in saying that every window it lists can be picked. On a Mac: the panel's window list (titles need the Screen & System Audio Recording permission; the app's own windows are left out), a window recorded with its app's sound through ScreenCaptureKit, and the entire screen and a window with the microphone left out, whose track should be silent with the microphone indicator off.

Run on one Mac on 2026-11-20 with e3ad51c installed, after fmt, clippy for macOS and Windows, the tests and the extension's tests passed there ([note](note-20261120-window-recording-by-hand-on-a-mac.md)). The user opened the window list and recorded windows of Chrome and Finder with and without the microphone, and the entire screen without it. Every file had the picture, two audio tracks and the `Manual` metadata. The microphone's track held sound only with the switch on. A window's track held its app's sound, also from another of its windows, though only Chrome's main process was named to the capture, and not another app's. macOS showed no microphone indicator, also while the microphone was recorded, so the indicator cannot tell whether the microphone was opened. The entire screen from the panel with the microphone on, and a window that closes, were not run.

### After the merge

- **Canvas.** The Windows session fixed the canvas from the target's size when the recording opened (at most 1920x1080, 1080x1920 for a tall window). The merge keeps the configured `video.width` by `video.height` instead, because a window can change shape after the start (a Slack huddle turned from portrait to landscape on joining). `recording/fit` scales a larger picture down, crops one a pixel larger, and puts a smaller one in the middle at its own size without scaling it up, as the user chose for Windows.
- **Audio.** The Windows session's write loop (each interval's sound written one step later, the mixer's depth control, sound captured before the loop dropped) runs once per audio track; each track is filled from its own source only.
- **Signals and windows.** The Windows session's rule of reading the processes and windows only while some process uses the microphone or an app's meeting is recorded is kept as it was. The user decided that the windows are not looked at while no microphone is in use. The detector no longer falls back to the app's largest window: a meeting's window is one its app's adapter tells is a meeting's, or else one that appeared after the windows began to be read; until there is one, the picture is black.
- **Meeting app adapters.** What tells an app's windows apart lives in adapters under `crates/app/src/apps` ([ADR](../adr/adr-20261119-tell-meeting-windows-apart-through-app-adapters.md)). Slack's reads the "[Main]" mark Slack puts in its main window's title while a joined huddle is open; on macOS that was observed in five huddles, from the moment of joining, never during the preview.
- **End of a meeting.** A desktop meeting ends when its app has not used the microphone for the end grace, whatever windows it still has ([ADR](../adr/adr-20261119-end-a-desktop-meeting-on-the-microphone.md)). On macOS Slack hid its huddle window on leaving and destroyed it 0.45 to 27.6 s later; while a window kept meetings going that delayed the end by 17 to 19 s.
- **Recording metadata.** Every finished MP4 is rewritten once to carry `com.meeting-assistant.app`, `started_at` and the meeting's metadata as QuickTime keys ([ADR](../adr/adr-20261119-write-meeting-metadata-into-the-recording.md)). The rewrite moves `stco`, `co64`, explicit `tfhd` base offsets and `tfra` past the `moov`; Media Foundation's fragmented output has not been rewritten on Windows. Check a desktop and a tab recording with `ffprobe -show_entries format_tags`, and that every packet is unchanged.
- **The macOS window list.** It now lists windows on screen only, so that a hidden window is never chosen to record. The Windows list already leaves out windows that are not visible (`IsWindowVisible`) and keeps minimized ones; nothing changes there.

### Agreed direction and limits

- No compatibility with older app or extension versions, and no fallbacks for them.
- Video is for context only; recordings are analysed into minutes. Nothing above 1080p. On Windows a smaller window is not scaled up; on macOS ScreenCaptureKit's own fitting is kept, as the user chose.
- The app's main window is never recorded; the picture stays black until the meeting's own window is found. The detector names no app; what is particular to an app lives in its adapter.
- A desktop meeting starts, continues and ends on its app's microphone use; no window keeps it going.
- Keep the boundaries in AGENTS.md: deciding code in `recording/`, `apps/` and `crates/core`, OS calls under `platform/win`.
- `--install` changes the user's registry, Start Menu and Desktop; run it only when asked, and stop a running copy with `--quit` first.

### Reading list

[AGENTS.md](../../AGENTS.md), [the capture pipeline design](../design/design-capture-pipeline.md), [the browser tab recording design](../design/design-browser-tab-recording.md), [the meeting detection design](../design/design-meeting-detection.md), the ADRs on [meeting app adapters](../adr/adr-20261119-tell-meeting-windows-apart-through-app-adapters.md) and [ending on the microphone](../adr/adr-20261119-end-a-desktop-meeting-on-the-microphone.md), and the verification notes of 2026-11-19: [macOS capture](note-20261119-macos-verification-2026-11-19.md), [two audio tracks](note-20261119-two-audio-tracks-verification-2026-11-19.md), [meeting window and end detection](note-20261119-meeting-window-and-end-detection-verific.md), [Slack huddle window after the merge](note-20261119-slack-huddle-window-after-the-merge.md) and [macOS idle resources](note-20261119-macos-idle-resources-after-the-merge.md).
