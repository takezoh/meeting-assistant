---
id: note-20261119-tab-capture-start-order-checked
kind: note
title: Tab capture start order checked in Chrome, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations: []
source_paths:
- extension/background.js
- extension/offscreen.js
summary: With the stream ID asked for last, a Google Meet tab recording started and
  finished normally in Chrome on one Mac; the failure that kept a capture pending was
  not reproduced.
---

## Summary

On 2026-11-19, on one Mac (macOS 26) with Chrome 157, the extension that asks for the tab's stream ID only after the recording page exists and the tab's size was read, and that stops captured tracks on every failure ([FAIL-004 of the tab recording design](../design/design-browser-tab-recording.md)), recorded one Google Meet call normally. Its failure paths are covered by `extension/test`; none of them was made to happen in a browser, and the capture the browser kept pending after the fake-UI switch on 2026-11-18 was not reproduced.

## Notes

### The call

| Time | Event |
| --- | --- |
| 20:45:09.734 | the meeting was detected |
| 20:45:09.940 | the tab recording had started (0.2 s later) |
| about 20:45:14.0 | left: the page changed and the tab played the leave sound, 4.3 s into the recording |
| 20:45:15.781 | the recording was saved with the microphone as its second audio track and its metadata written |

The file held the tab's audio, its video at 2880x1566 and the microphone, 5.8 s long, with `com.meeting-assistant.app`, `started_at`, `meeting.meeting_code` and `meeting.page_title`. No `.microphone.m4a`, `.merging` or `.annotating` file was left. The extension had been reloaded in Chrome after the install and the Meet tab opened again.

### What was not done in a browser

- A start that fails after the tab was captured, a start while recording, and a capture held by the browser: these are checked only by `extension/test`, against fakes of the browser's APIs.
- Whether Chromium leaves a tab's capture pending after an ordinary `getUserMedia` failure. Chromium's `tab_capture_registry.cc`, read on 2026-11-19, keeps a pending or running capture until its media request reports an end, stop or error, the tab closes or the extension unloads. Loading a probe extension into a throwaway Chrome profile to watch it failed twice: Chrome 157 ignored `--load-extension`, and the DevTools pipe closed before `Extensions.loadUnpacked` could be sent.
