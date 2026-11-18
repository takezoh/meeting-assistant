---
id: note-20261118-macos-recording-and-os-microphone-handof
kind: note
title: macOS recording and OS microphone handoff
status: draft
created: '2026-11-18'
topic: handoff
summary: Where the macOS desktop recorder stands (implemented; writer and microphone
  run on a Mac; ScreenCaptureKit capture not run) and what is open for putting the
  OS microphone into browser tab recordings.
tags: []
owners: []
relations:
- {type: references, target: adr-20261118-capture-on-macos-with-screencapturekit-a}
- {type: references, target: issue-20261117-macos-has-no-native-window-display-or-ap}
- {type: references, target: issue-20261118-the-extension-cannot-capture-the-microph}
- {type: references, target: design-capture-pipeline}
source_paths:
- crates/app/src/platform/mac/recorder/mod.rs
- crates/app/src/platform/mac/recorder/content.rs
- crates/app/src/platform/mac/recorder/stream.rs
- crates/app/src/platform/mac/recorder/writer.rs
- crates/app/src/platform/mac/recorder/microphone.rs
- crates/app/src/platform/mac/signals.rs
- crates/app/src/recording/pcm.rs
- crates/app/src/recording/pipeline.rs
- extension/offscreen.js
- extension/options.js
---

## Summary

Handoff for two pieces of work that follow each other. The macOS `CaptureBackend` (ScreenCaptureKit, AVAssetWriter, AVAudioEngine) is implemented and its writer and microphone have run on a Mac, but no window, display or app audio has been captured yet, because that needs the Screen & System Audio Recording permission for the app. Putting the user's voice into browser tab recordings through the operating system's microphone is agreed in direction but not designed; three decisions are the user's.

This is a handoff, not a contract. The decision for the macOS recorder is in [the macOS capture ADR](../adr/adr-20261118-capture-on-macos-with-screencapturekit-a.md); the defect it follows up is [the extension microphone issue](../issue/issue-20261118-the-extension-cannot-capture-the-microph.md); the remaining verification is tracked by [the macOS capture issue](../issue/issue-20261117-macos-has-no-native-window-display-or-ap.md).

## Goal

- Desktop meeting apps and manual recording work on macOS as they do on Windows, observed with a real call.
- Browser tab recordings contain the user's voice, on macOS and Windows.

## Observed facts

All on one Apple Silicon Mac with macOS 26 on 2026-11-18, from the commit that adds this note.

| Observation | Source |
| --- | --- |
| AVAssetWriter finished a two-second MP4 of generated pictures and a tone; AVFoundation reports it playable, video 640x360 and audio, both 2.0 s | `the_writer_finishes_an_mp4_with_video_and_audio` in `platform/mac/recorder/mod.rs`, file probed with AVURLAsset |
| H.264 honours `AVVideoQualityKey`: two seconds of 640x360 noise came to 2.9 MB at quality 30 and 23.5 MB at quality 90 | A temporary test, removed afterwards |
| AVAudioEngine delivered microphone sound converted to the mixer's 48 kHz stereo 16-bit format | `the_microphone_delivers_sound_in_the_mixer_format` (ignored by default), run by hand from Terminal |
| Terminal.app, the process responsible for commands run in this session, has the microphone permission and not the screen recording permission | `CGPreflightScreenCaptureAccess()` false, `AVCaptureDevice.authorizationStatus(for: .audio)` authorized; neither call prompts |
| `--probe` lists processes and windows through Core Audio process objects, libproc and CGWindowList | Run earlier the same day |
| In Chrome 157 the extension's offscreen document is refused the microphone although its permission reads `granted` | [The extension microphone issue](../issue/issue-20261118-the-extension-cannot-capture-the-microph.md) |

ScreenCaptureKit was deliberately not run from Terminal: macOS would have asked the screen recording permission for Terminal, which would give it to everything started from it.

## Agreed direction

- The user asked for macOS desktop recording and the microphone with deciding code separate from acting code, ports with real adapters and fakes. The macOS modules hold OS calls only; float audio conversion and resampling are pure (`recording::pcm`).
- For the browser microphone the user asked whether to take it from the OS, and the OS route was recommended: the app records the microphone itself while the extension records the tab. The macOS microphone adapter needs only the microphone permission, so it can serve tab recordings.
- No compatibility with older Chrome, older extensions or older app versions, and no fallbacks for them. This does not relax the product rule against widening a recording when the app's audio is unavailable.
- Shared pipeline fix made in the same commit: a failure while writing still finalizes the file (`design-capture-pipeline` INV-003). This also changes Windows.

## Open decisions for the user

1. **Running `--install` for the live check.** It replaces the installed copy that is running. The user then turns on Meeting Assistant under Screen & System Audio Recording and Microphone in System Settings and restarts it. AGENTS.md allows `--install` only when asked.
2. **How the microphone joins a tab recording.** Mixing it into the tab's audio track (recommended: every player plays both voices; audio is re-encoded, video is copied) or adding it as a second audio track (no re-encoding; some players play only one track).
3. **Removing the extension's microphone code** in `extension/offscreen.js` and the microphone button and explanations in `extension/options.js` and `options.html`, once the OS route works.

## Next steps

1. With the user's go-ahead: `cargo build --release`, `meeting-assistant --quit`, then `target/release/meeting-assistant --install`, then the user gives the two permissions and restarts the app.
2. `open -n -a "Meeting Assistant" --stdout <log> --args --self-test <path>` and probe the file. Started through `open`, macOS attributes the permissions to the app, not to the terminal.
3. Record the entire screen from the status panel, then a real call in Zoom, Teams or Slack. Check that the recording has the other side's voice. If it is silent, the app-audio filter may not include the meeting app's helper processes; `--probe` shows which process uses the output.
4. Check whether the ad-hoc signature makes macOS ask again or list the app twice after the next update.
5. Write a dated note with the outcome and close the macOS capture issue against it.
6. After decision 2, design the tab-recording microphone with the `design` skill before implementing it.

## Design questions for the tab-recording microphone

These are for the design step, not decided here:

- Which component starts and stops the microphone with a tab recording. The recording hub and server live in `crates/app/src/browser/`; the microphone adapters in `platform/mac/recorder/microphone.rs` and `platform/win/recorder/audio.rs`.
- How the microphone and the tab recording are aligned in time. The upload starts when the extension's WebSocket connects; the extension's `MediaRecorder` starts slightly earlier.
- Where the microphone is kept until the tab recording ends (an AAC side file, or PCM in memory), and the port that merges the two files. Merging decodes and re-encodes audio, which is platform code (AVFoundation, Media Foundation) behind a port; the timing and mixing decisions stay pure.
- What a failure to merge leaves behind. The tab recording must stay intact; the microphone file must not be lost silently.
- Whether the tab is MP4 (H.264/AAC) in every supported Chrome and Edge. The extension chooses the container; the app names the file after it.

## Non-goals

- Recording several meetings in parallel (declined by the user).
- Detecting browser meetings without the extension (removed on all platforms).
- Windows performance work; it has its own [handoff](note-20261118-windows-recording-performance-handoff.md).

## Other open items

- A test runner for the extension's JavaScript (Node's built-in test runner over pure functions such as `meetingTitle`) was proposed in the structure review and not started.
- macOS 14.2 is stated as the minimum from the APIs used; the app has run only on macOS 26.
- The loopback listeners now admit only the current user's connections ([ADR](../adr/adr-20261118-admit-loopback-connections-only-from-the.md)). The macOS lookup ran in its adapter test; no real browser has connected through it yet, because the installed copy is an older build. The Windows lookup is type-checked only: run `cargo test -p meeting-assistant peers` and a tab recording on Windows.
