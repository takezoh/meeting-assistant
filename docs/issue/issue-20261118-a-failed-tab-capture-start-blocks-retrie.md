---
id: issue-20261118-a-failed-tab-capture-start-blocks-retrie
kind: issue
title: A failed tab capture start blocks retries for the same tab
status: open
created: '2026-11-18'
issue_type: bug
tags: []
owners: []
relations: []
subject_paths:
- extension/background.js
- extension/offscreen.js
summary: After one failed attempt to start a tab recording, every retry for the same tab fails with "Cannot capture a tab with an active stream", so the meeting is not recorded.
---

## Symptom

When the first `getUserMedia` call for a tab capture fails in the offscreen document, the app's retries for the same meeting fail with "Cannot capture a tab with an active stream." The coordinator gives up after its third attempt and the meeting is not recorded.

## Evidence

Observed on macOS on 2026-11-18 with Chrome 157 in a throwaway profile (see the [macOS verification note](../note/note-20261118-macos-verification-2026-11-18.md)). The first failure was caused by the test switch `--use-fake-ui-for-media-stream`, which refuses tab capture with "Requested device not found". The app log then showed:

```
ERROR Could not start recording (1/3): The extension cannot record the tab: Requested device not found
ERROR Could not start recording (2/3): The extension cannot record the tab: Cannot capture a tab with an active stream.
ERROR Could not start recording (3/3): The extension cannot record the tab: Cannot capture a tab with an active stream.
```

`startRecording` in `extension/background.js` obtains a stream ID with `chrome.tabCapture.getMediaStreamId` for every attempt; nothing releases the capture that the failed attempt registered for the tab.

## Context for follow-up

The trigger here was a test switch, but any failure of the first `getUserMedia` call leaves the tab in this state. The retry policy is in the coordinator and is governed by the [browser tab recording design](../design/design-browser-tab-recording.md). Whether the browser releases the pending capture after a timeout was not observed.
