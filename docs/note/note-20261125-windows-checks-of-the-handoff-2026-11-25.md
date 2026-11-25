---
id: note-20261125-windows-checks-of-the-handoff-2026-11-25
kind: note
title: Windows checks of the handoff on 2026-11-25
status: draft
created: '2026-11-25'
tags: []
owners: []
relations:
- {type: references, target: note-20261119-windows-handoff-after-the-macos-work-of-}
- {type: references, target: note-20261119-windows-checks-after-the-macos-merge}
- {type: references, target: adr-20261124-write-recordings-in-progress-apart}
- {type: references, target: adr-20261119-tell-meeting-windows-apart-through-app-adapters}
- {type: references, target: issue-20261125-tab-recordings-are-taken-above-1080p}
source_paths:
- crates/core/src/handover.rs
- crates/app/src/adapters/mod.rs
- crates/app/src/apps/slack.rs
- extension/background.js
- tools/avsync/src/main.rs
summary: 'What was run on one Windows PC on 2026-11-25 with 9c5ce64: the in-progress
  folder, a Slack huddle and a Google Meet tab recording with the installed app, and
  what remains.'
---

## Summary

Observed on 2026-11-25 on one PC, by the user and Claude Code: Windows 11 (build 26200) with an NVIDIA GeForce RTX 4060 Ti and a 3840x2160 display at 125%, used at its console. The build was the release build of 9c5ce64, run from the checkout with a configuration of its own and, for the meetings, installed with `--install`. The checks follow [the Windows handoff](note-20261119-windows-handoff-after-the-macos-work-of-.md) and add the Windows side of writing recordings in progress apart ([ADR](../adr/adr-20261124-write-recordings-in-progress-apart.md)).

- The tests and the eight ignored Windows tests passed.
- Writing a recording in `in-progress`, moving it out when it stops, when the app quits and at the next start after the app was killed, and recovering files left there beforehand, all behaved on Windows as on the Mac.
- A Slack huddle was recorded black until it was joined, then with the huddle window only. Slack on Windows marks its main window "[Main]" while a joined huddle is open, as on macOS.
- A Google Meet tab recording had the app's microphone recording added as its last track, and nothing was left beside it. Its video was 2210x1488, above 1080p ([the issue](../issue/issue-20261125-tab-recordings-are-taken-above-1080p.md)).

No voice was recorded: the only active microphone was the Oculus virtual headset microphone, muted in Windows settings. Teams, Zoom and Webex were not run.

## Notes

### Run first

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace`: passed, 262 app tests with 10 ignored. `node --test "extension/test/*.test.mjs"`: 10 passed.
- `cargo clippy --workspace --target aarch64-apple-darwin` failed on this PC in the build script of `objc2-exception-helper`, which compiles C for the Apple target. The macOS code was not type-checked.
- The eight ignored Windows tests (`cargo test -p meeting-assistant -- --ignored`, without the two that read files named in the environment): passed.

### How the files were read

ffprobe was not available. `tools/avsync` gained a `tracks` command that lists a file's tracks in the order the file stores them and its metadata keys. It then decodes each track with Media Foundation and prints, per audio channel, its peak and when it first goes above -40 dBFS, and for the video where the picture is not black. Media Foundation lists a file's streams in another order than its tracks, and its stream identifiers are not the track IDs, so the command matches each stream to the track whose first sample sizes it reads.

### Self-test

A 10-second `--self-test` while a 440 Hz tone played on the left channel only:

- The 3840x2160 display was fitted into a 1920x1080 picture, encoded by `NVIDIA H.264 Encoder MFT`; 301 pictures.
- Track 2, the other side's: the tone on the left only (peak 8086, the right at 2), loud from 1.70 s.
- Track 3, the microphone's: silent (peak 2). The log said "The microphone is muted in Windows settings."

### Recordings in progress apart

With the build run from the checkout and a recordings folder of its own. The panel's page was driven through WebView2's DevTools protocol by sending its own messages; the page let the debugging port go about 30 s after the app started.

- **Startup recovery.** Put in `in-progress` beforehand:
  - a recording with a partial `.annotating.mp4` copy;
  - an empty `.mp4`;
  - a recording with its `.microphone.m4a`, whose name was already taken in the recordings folder.

  At startup the first recording was moved with the same SHA-256 and its copy removed. The empty file was moved. The third recording and its microphone file were moved together as `_2`. The folder was removed. The log had one warning per recording, "Moved a recording left in progress by an earlier run".
- **Stopped by quitting.** An entire-screen recording with the microphone, written in `in-progress`, was stopped by `meeting-assistant --quit` after 30 s. It was finalized, its metadata (`Manual` and its start) written, and moved out.
- **Stopped from the panel.** An entire-screen recording without the microphone was moved out when the panel's Stop message was sent. The log named the microphone source as silence and gave no microphone warning, so the microphone was not opened.
- **Killed.** A recording ended with `taskkill /F` 14 s in stayed in `in-progress`. It had no fragment index, and Media Foundation decoded 13.2 s of all three tracks. The next start moved it, with the warning.

### The warning for a sound that is not recorded

No sound failed to open in any run, so the warning was not seen on Windows. Setting `HKCU\...\CapabilityAccessManager\ConsentStore\microphone\NonPackaged` `Value` to `Deny` with `reg.exe` did not stop a self-test from opening the microphone and capturing 5.2 s. The setting was not changed through Settings, and this app does not appear among the desktop apps whose microphone use Windows records. Whether the change took effect is therefore unknown. The value was set back to `Allow`.

### A Slack huddle

The installed app, with the user's configuration (`end_grace_seconds = 15`). Slack is the Microsoft Store app. Its microphone access had been turned off in Windows settings since 2026-10-16; the user turned it on first. The user started a huddle in a channel of their own, joined it, muted for a while and left. A script logged Slack's top-level windows every 200 ms and ran `--probe` about once a second.

| Time | Window | Microphone | App |
| --- | --- | --- | --- |
| 10:38:06.7 | A window titled "- thirdverse - Slack" appears | | |
| 10:38:08.1 | | First probe that lists Slack | |
| 10:38:08.3 | | | Starts reading the windows |
| 10:38:12.6 | | | Recording starts with no window: black |
| 10:38:21.4 | The window is retitled "testtest-gn - thirdverse - Slack" | A probe lists no Slack process | |
| 10:38:21.6 | The main window's title gets "[Main] 🏠" (later also "🔊") | | |
| 10:38:22.4 | | | Moves to the huddle window |
| 10:39:08.3 | The huddle window is destroyed and the mark goes | | |
| 10:39:24.3 | | | Meeting end detected and recording saved |

- The huddle window appeared before Slack was seen with the microphone and before the windows were read. Only the "[Main]" mark told it apart, as on macOS.
- The picture was black for 9.77 s. It then showed a box in the middle of the canvas, 760x448 and then 760x544: the huddle window at its own size. Slack's main window, 2398x1289 and so scaled down to the canvas's full width, never appeared.
- Track 2 held Slack's sound from 8.91 s, when the huddle was joined. Track 3, the microphone's, was silent.
- The metadata had `conversation` `testtest-gn` and `workspace` `thirdverse`, from the huddle window's title.
- The recording ended 16 s after the huddle window was destroyed, with an end grace of 15 s.
- The probe that ended at 10:38:21.4, as the huddle was joined, listed no Slack process using the microphone; the one before, at 10:38:13, did. The script failed on the next probe's output, so later probes were not recorded. Whether Slack lets go of the microphone on joining, and for how long, is not known. With the default end grace of 2 s such a gap could end the recording when the huddle is joined. Whether Slack keeps the microphone while muted on Windows is not known either; with 15 s the recording went on through the mute.
- Slack's main window on Windows carries the unread count in its title, as in "ai-data-business (Channel) - thirdverse - 4 new items - Slack". The adapter reads `workspace` as "4 new items" from such a title. The huddle window's title had no count in this huddle; whether it ever has one is not known.

### A Google Meet tab recording

The user quit Chrome, started it from the "Google Chrome (Meeting Assistant)" launcher, reloaded the extension and started a Google Meet call alone, which they left after about 30 s. The app used the user's configuration (`browser_end_grace_seconds = 5`).

- The meeting was detected at 10:42:56.3. The app then recorded the microphone alone, without video.
- The end was detected at 10:43:31.3, about 6 s after the leave sound in the tab's audio.
- The browser uploaded an MP4 of 1,313,814 bytes (`avc1.640028`, `mp4a.40.2`). The log said the voice was added as the second audio track. No `.microphone.m4a` was left, and the file was in the recordings folder.
- The file held:
  - the tab's audio as track 1;
  - its video as track 2, at 2210x1488, with 356 pictures in 30 s;
  - the microphone as track 3, the last, silent.
- The microphone's first sample was 5 ms after the tab audio's.
- It carried `GoogleMeet`, its start, `meeting_code` and `page_title`.

### Not checked

- A voice in any recording, and how the microphone lines up with the other side in a tab recording.
- Whether Slack lets go of the microphone when a huddle is joined or muted; the recordings with the default end graces.
- Teams, Zoom and Webex.
- Edge.
- Whether the warning notifications appeared on screen.
