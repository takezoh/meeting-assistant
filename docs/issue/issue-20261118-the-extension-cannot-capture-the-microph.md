---
id: issue-20261118-the-extension-cannot-capture-the-microph
kind: issue
title: The extension cannot capture the microphone in its offscreen document
status: open
created: '2026-11-18'
issue_type: bug
tags: []
owners: []
relations: []
subject_paths:
- extension/offscreen.js
- extension/options.js
summary: Tab recordings never contain the user's voice in Chrome 157 on macOS; the offscreen document's getUserMedia
  for the microphone is refused although the extension origin's permission is granted.
---

## Symptom

Tab recordings contain the tab's sound but not the user's voice. When a recording starts, the offscreen document asks for the microphone and is refused, and the app logs the extension's warning. The microphone mix inside the extension has never been observed working: neither the Windows verification of 2026-11-17 nor the macOS verification of 2026-11-18 records it.

## Evidence

On a Mac on 2026-11-18, Chrome 157, the user's profile started from the launcher, with the extension's site setting Microphone set to Allow:

```
WARN  Warning from the extension: Your own voice will not be recorded because the microphone could not be captured (NotAllowedError: Permission denied; microphone permission: granted). ...
```

`navigator.permissions.query({ name: "microphone" })` in the offscreen document returns `granted`, and `navigator.mediaDevices.getUserMedia({ audio: true })` in the same document rejects with `NotAllowedError: Permission denied`. The offscreen document is created with the reason `USER_MEDIA` and first opens the tab-capture stream with `getUserMedia` and a `chromeMediaSource: "tab"` constraint.

Related: with the site setting at Ask (default), the options page's **Allow microphone** also failed at once with `NotAllowedError: Permission denied`, without a prompt, in two profiles. A Chromium Extensions thread from 2023–2024 reports offscreen microphone capture working once the site setting was set to Allow, which does not match what was observed here.

A second build asked for the microphone before opening the tab-capture stream. At 20:23 on the same day it was refused the same way (`NotAllowedError: Permission denied; microphone permission: granted`), so the order of the two requests is not the cause.

## Context

The extension's recording design is in [the browser tab recording design](../design/design-browser-tab-recording.md); the observations are in [the macOS note](../note/note-20261118-macos-verification-2026-11-18.md). Whether the refusal is specific to offscreen documents or to this Chrome version has not been tested.
