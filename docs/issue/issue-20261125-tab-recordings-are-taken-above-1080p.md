---
id: issue-20261125-tab-recordings-are-taken-above-1080p
kind: issue
title: Tab recordings are taken above 1080p
status: open
created: '2026-11-25'
issue_type: bug
tags: []
owners: []
relations:
- {type: references, target: design-browser-tab-recording}
- {type: references, target: note-20261125-windows-checks-of-the-handoff-2026-11-25}
- {type: references, target: note-20261119-tab-capture-start-order-checked}
subject_paths:
- extension/background.js
- extension/offscreen.js
summary: The extension caps a tab's capture at the tab's size in device pixels, not
  at 1920x1080, so a tab is recorded at up to its full resolution.
---

## Symptom

A browser tab is recorded at the size of its page in device pixels, which on a high-resolution display is larger than 1920x1080. Desktop and screen recordings are fitted into `video.width` by `video.height`, at most 1920x1080. The agreed direction in [the Windows handoff](../note/note-20261119-windows-handoff-after-the-macos-work-of-.md) is that video is for context only and nothing is recorded above 1080p.

## Evidence

- On one Windows PC on 2026-11-25, with a 3840x2160 display at 125%, a Google Meet tab recorded in Chrome had its video at 2210x1488 ([the note](../note/note-20261125-windows-checks-of-the-handoff-2026-11-25.md)).
- On one Mac on 2026-11-19 a Google Meet tab recording had its video at 2880x1566 ([the note](../note/note-20261119-tab-capture-start-order-checked.md)).
- `extension/background.js` passes the tab's width and height multiplied by the page's device pixel ratio as the upper limit. `extension/offscreen.js` uses them as `maxWidth` and `maxHeight` of the capture, with no further bound.

## Context for follow-up

- [The browser tab recording design](../design/design-browser-tab-recording.md) owns the extension's capture. [PRODUCT.md](../../PRODUCT.md) states the 1920x1080 bound under the picture of a recording without saying whether it covers tab recordings.
- The extension's tests (`extension/test`) check the order in which a start passes the tab's size, and can check a bound without a browser.
- Tab recordings are copied as the browser wrote them; the app does not re-encode them, so a bound has to be set where the browser captures.
