---
id: note-20261119-meeting-window-and-end-detection-verific
kind: note
title: Meeting window and end detection verification, 2026-11-19
status: published
created: '2026-11-19'
topic: verification
summary: Slack huddle signals observed with --probe, how soon recordings ended after leaving a Slack huddle and a
  Google Meet call once the meeting window and shorter grace periods were in. Dated observations, not guarantees.
tags: []
owners: []
relations:
- {type: references, target: design-meeting-detection}
source_paths:
- crates/core/src/detector.rs
- crates/core/src/engine.rs
- extension/hook.js
- crates/app/src/diagnostics.rs
---

## Summary

On 2026-11-19, on one Mac with macOS 26, Slack (desktop) and Chrome 157, the user reported that a Slack huddle was recorded as Slack's main window and that recordings ended later than PLAUD's. Slack huddles were probed state by state, the detector was changed to record and follow the window that appears with a meeting, the grace periods were shortened, and a huddle and a Google Meet call were recorded again. Only Slack and Google Meet were tried, each once after the change, and nothing on Windows.

## Notes

### Before the change

A 23-minute huddle recorded Slack's main window (2880x1740 pixels) and ended 14.6 seconds after the huddle's audio stopped, the desktop end grace of 15 seconds. Slack's audio, which its helper process plays, was captured although only the main process was named to ScreenCaptureKit: 66,233,280 frames, exactly 48 kHz over the recording, loudest 0 dBFS.

### Slack huddle, probed by state

`--probe` (extended to list the windows of meeting apps with their titles) was run through `open` while the user was in each state; a separate script listed Slack's windows on every layer.

| State | Microphone | Audio output | Windows |
| --- | --- | --- | --- |
| Talking | Slack Helper | Slack Helper | main window titled "… (Channel) - tkz - Slack [Main] 🏠", and a second window "dev-gcp-errors - tkz - Slack" (the huddle), both on layer 0 |
| Muted | Slack Helper (kept) | Slack Helper | the same |
| Just after leaving | released at once | still on | the huddle window gone; "[Main] 🏠" gone from the main window's title |
| About 10 s later | none | none | main window only |

PLAUD's process also held the microphone during the huddle and released it at the same moment as Slack.

### After the change

The build with the meeting window and the new defaults (desktop end grace 2 s, browser end grace 1 s, a reconnecting call counted as connected) was installed, and the user's `config.toml`, which held the old values, was changed to them.

| Meeting | Recorded | From leaving to the end of the recording |
| --- | --- | --- |
| Slack huddle, 26 s, with a mute | the huddle window, 1048x1520 pixels | about 2.7 s (leave sound and picture change at 22.9 s, end at 25.7 s) |
| Google Meet, 90 s | the tab, with the microphone as the second audio track | about 3.1 s (the page changed at 86.7 s, end at 89.8 s) |

### Picture size

In that huddle recording the picture was 1048x1520 pixels, taken from the huddle window before joining; when joining turned the window landscape, the picture shrank to 1044x612 of it, about 60% black. The canvas was then fixed at `video.width` by `video.height` (1920x1080 by default). In two more huddles recorded with that build, the picture was 744x1036 of the 1920x1080 canvas before joining and 1852x1080 after, anchored at the top left by ScreenCaptureKit with the margin on the right. The two huddles gave two recordings, as they should. Windows has not been run.

Reloading the extension during a first Meet call lost its connection: the recording failed and kept its partial file with the microphone added, three retries on the lost connection failed and left no microphone files, and the call was recorded again once the reloaded extension reported it.

### Not checked

- An app that releases the microphone while muted, where the meeting window is what keeps the recording going. Slack keeps the microphone.
- Zoom, Teams and Webex, including whether their call windows close when a call ends.
- A browser call that drops and reconnects.
