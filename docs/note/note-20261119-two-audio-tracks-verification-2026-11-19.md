---
id: note-20261119-two-audio-tracks-verification-2026-11-19
kind: note
title: Two audio tracks verification, 2026-11-19
status: published
created: '2026-11-19'
topic: verification
summary: What was run for two audio tracks and the app-recorded microphone in tab recordings on one Mac on 2026-11-19,
  and what was not. Dated observations, not guarantees.
tags: []
owners: []
relations:
- {type: references, target: note-20261119-two-audio-tracks-in-every-recording-desi}
- {type: references, target: design-capture-pipeline}
- {type: references, target: design-browser-tab-recording}
- {type: references, target: issue-20261119-every-update-drops-the-macos-screen-reco}
source_paths:
- crates/app/src/recording/mp4.rs
- crates/app/src/recording/merge.rs
- crates/app/src/linked.rs
- extension/offscreen.js
---

## Summary

On 2026-11-19, on one Apple Silicon Mac with macOS 26, Chrome 157 and Rust 1.100, the change that keeps the other side and the microphone in two audio tracks, and records the microphone in the app during a browser tab recording, was tested automatically and once in a real Google Meet call. The call had no other participant and lasted 15 seconds. The timing of the microphone against the other side was not checked, and nothing was run on Windows.

The plan is in [the design note](note-20261119-two-audio-tracks-in-every-recording-desi.md).

## Notes

### Automated

- `cargo test --workspace`: all passed on macOS, including the macOS writer tests (a file with video and two audio tracks, and an audio-only file, parsed for their tracks) and the MP4 track tests.
- `cargo clippy --workspace --all-targets`, `--target aarch64-apple-darwin` and `--all-targets --target x86_64-pc-windows-msvc`: no warnings. The Windows target was type-checked only; the `x86_64-pc-windows-msvc` standard library was added to the toolchain for it.
- `cargo fmt --all --check` and `docs lint`: clean.

### Merging into a real browser recording

A copy of a 13-minute Google Meet recording made by Chrome 157 (fragmented MP4, 391 fragments, audio track 1 at 48 kHz, video track 2 at timescale 30000) was merged with a generated 14-minute AAC tone.

| Remuxer | Video packets, last time | Tab audio packets, last time | Time |
| --- | --- | --- | --- |
| Original file | 20,296, 791.359 s | 37,090, 791.232 s | |
| AVFoundation passthrough export (rejected) | 20,296, 781.758 s | 37,088, 790.550 s | under 1 s |
| `recording::mp4` (adopted) | 20,296, 791.359 s | 37,090, 791.232 s | under 1 s |

With `recording::mp4` the tone was only in the second audio track (1 kHz at -24 dB there, absent from the first), the added track ended at the recording's end, AVFoundation reported the file playable, and ffmpeg decoded every track. ffmpeg reported 20 non-monotonic video timestamps when decoding both the original and the merged file; they come from the browser's recording.

### Real Google Meet call

The build was installed with `--install`, the extension files were replaced, and the user reloaded the extension, joined a Meet call alone, spoke and left.

| Observed | Detail |
| --- | --- |
| Order | Meeting detected at 10:52:36.105; microphone recording ready at 10:52:40.252; tab recording started at 10:52:40.346; meeting end at 10:52:55.385; "Added your own voice to the recording as its second audio track." at 10:52:55.458 |
| File | Track 1 AAC (tab), track 2 H.264 video, track 3 AAC (microphone), 15.0 to 15.1 s each; no `.microphone.m4a` left beside it |
| Content | The tab's audio silent apart from one sound at 8 s (no other participant). The microphone track at about -70 dB with louder seconds; the user listened to it and heard their own voice |
| Start delay | The 4 seconds between detection and the microphone being ready were the macOS microphone prompt, which appeared as the call started: the update had reset the microphone permission too. The tab recording waits for the microphone, so the first seconds of that call were not recorded |

### Not checked

- The timing of the microphone against the other side's audio. It needs a sound that reaches both, for example a second device in the same call. The design estimate (tens of milliseconds) is untested.
- Calls with other participants, long calls, Edge, and recordings the browser writes as WebM.
- A desktop meeting app's window and audio with ScreenCaptureKit, and anything on Windows, including the Windows writer with two audio tracks.
- What happens when the process is killed during a tab recording; by design the tab's file and the microphone file stay separate.
