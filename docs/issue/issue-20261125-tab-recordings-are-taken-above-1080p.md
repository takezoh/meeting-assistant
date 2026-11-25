---
id: issue-20261125-tab-recordings-are-taken-above-1080p
kind: issue
title: Tab recordings are taken above 1080p
status: done
created: '2026-11-25'
issue_type: bug
tags: []
owners: []
relations:
- {type: references, target: design-browser-tab-recording}
- {type: references, target: note-20261125-windows-checks-of-the-handoff-2026-11-25}
- {type: references, target: note-20261119-tab-capture-start-order-checked}
- {type: references, target: note-20261125-tab-capture-sizes-in-chrome-and-edge}
subject_paths:
- extension/background.js
- extension/offscreen.js
summary: 'Fixed on 2026-11-25: the app sends video.width and video.height with start,
  and the extension captures the tab at its size in device pixels scaled down into
  them. Checked in Chrome with a test tab; not yet with the app on a meeting page.'
disposition_target: design-browser-tab-recording
updated: '2026-11-25'
---

## Symptom

A browser tab is recorded at the size of its page in device pixels, which on a high-resolution display is larger than 1920x1080. Desktop and screen recordings are fitted into `video.width` by `video.height`, at most 1920x1080. The agreed direction in [the Windows handoff](../note/note-20261119-windows-handoff-after-the-macos-work-of-.md) is that video is for context only and nothing is recorded above 1080p.

## Evidence

- On one Windows PC on 2026-11-25, with a 3840x2160 display at 125%, a Google Meet tab recorded in Chrome had its video at 2210x1488 ([the note](../note/note-20261125-windows-checks-of-the-handoff-2026-11-25.md)).
- On one Mac on 2026-11-19 a Google Meet tab recording had its video at 2880x1566 ([the note](../note/note-20261119-tab-capture-start-order-checked.md)).
- `extension/background.js` passes the tab's width and height multiplied by the page's device pixel ratio as the upper limit. `extension/offscreen.js` uses them as `maxWidth` and `maxHeight` of the capture, with no further bound.
- On 2026-11-25, in Chrome 158 and Edge 158 on Windows, a probe that computed and passed the size the same way had a tab captured and recorded at 2232x1446 and 2222x1450 ([the note](../note/note-20261125-tab-capture-sizes-in-chrome-and-edge.md)).

## Cause

Read from the code at ae5f4cf on 2026-11-25. The bound never reaches the extension, and the extension sets none of its own:

1. **The app sends no size.** `config.video` goes only to the two platform recorders (`crates/app/src/composition.rs`, `build_engine`). `BrowserLinkServer::start` takes no configuration, and the hub's `start` command is `Outgoing::Start { tab, upload, token }` (`crates/app/src/browser/protocol.rs`, built in `crates/app/src/browser/hub.rs`). The extension cannot know `video.width` and `video.height`.
2. **The extension limits the capture to the tab's own size.** `startRecording` in `extension/background.js` reads the tab's width and height with `chrome.tabs.get`, multiplies them by the page's `devicePixelRatio` that `bridge.js` reports, and passes that as `size`. Its comment says the limit is there because without one the capture takes the size of the screen.
3. **The capture takes that size as its only limit.** `start` in `extension/offscreen.js` puts `size` into the legacy constraints as `maxWidth` and `maxHeight`, beside `maxFrameRate: 30`. `MediaRecorder` gets only bit rates.
4. **Nothing after the capture changes the size.** The app copies the tab's fragments byte for byte and adds the microphone as a track ([the design](../design/design-browser-tab-recording.md), `recording::mp4`), so the picture the browser captures is the picture in the file.

The observed sizes fit the tab's size in device pixels: 2210x1488 is 1768x1190 at 1.25, and 2880x1566 is 1440x783 at 2. The tabs' sizes were not logged during those recordings, so this match is inferred. The probe's run did show the mechanism: the size passed was the size recorded.

The tab limit dates from the first commit (051cdab, 2026-11-17). The 1080p bound came on 2026-11-19 for the canvas of desktop and screen recordings ([the issue](issue-20261118-the-recording-canvas-has-no-upper-bound.md), ee9ea9c), and was not carried over to tab recordings. No test checks the size a start passes: `extension/test/background.test.mjs` checks only when the tab's size is read.

## Where a bound can go

| Option | Against the contracts |
| --- | --- |
| Scale the tab recording in the app | Contradicts the design: the tab's fragments are kept as the browser wrote them, and the microphone is added without re-encoding. It would also add a full re-encode after every browser meeting |
| A constant 1920x1080 in the extension | Bounds tab recordings at 1080p, but a smaller `video.width` and `video.height` would still not apply, and the bound would be written twice, once in `crates/core` and once in the extension |
| The app sends its bound with `start`, and the extension fits the tab's size in device pixels into it, keeping the aspect ratio | Makes PRODUCT.md's "the picture is `video.width` by `video.height` at most" true for tabs too, with one source of the bound. It changes the wire contract, which AGENTS.md allows when both sides change together. An extension that does not know the new fields ignores them and stays unbounded |

The third option fits. It would change:

- the `start` message, with two new fields in `protocol.rs` and [the browser link reference](../technical/browser-link.md);
- `BrowserLinkServer::start` and the hub, which need to be given the bound;
- `startRecording` in `background.js`, to fit the size into the bound, with a test in `extension/test`;
- PRODUCT.md and the design, which should say that a tab is recorded at most at that size.

The size to pass is the tab's size in device pixels fitted into the bound: scaled down with its aspect ratio kept, and never up, as `fitted` in `crates/app/src/recording/fit.rs` does for the desktop. The bound itself is the wrong value. In the run below, only a maximum fixed the pictures at that size, so passing 1920x1080 as it is added black bars and scaled a smaller tab up.

## Checked in a browser

[The note](../note/note-20261125-tab-capture-sizes-in-chrome-and-edge.md) has the runs in Chrome 158 and Edge 158 on one Windows PC on 2026-11-25. Both browsers behaved the same:

- **Only a maximum fixes the size.** With only `maxWidth` and `maxHeight`, every picture came at exactly that size. A smaller tab was scaled up into it, and a tab of another shape was centered with black bars. Equal minimum and maximum gave the same pictures. With no size, pictures came at the display's 3840x2160. "Only bounded" and "fixed" are therefore not two choices.
- **A fitted size gives the tab as it is, only smaller.** With the size fitted into 1920x1080, pictures came at that size and the tab filled them: 1668x1080 in Chrome and 1656x1080 in Edge for a tab of about 2230x1450 device pixels. A tab already within the bound kept its own size.
- **A resize during a recording keeps the size.** The pictures kept their first size, and the tab was fitted into them again with bars. After the window became smaller, the tab was scaled up to fill the width. The MP4 kept one sample description and `MediaRecorder` reported no error.

Not run: the repository's extension with the app, macOS, and a device pixel ratio other than 1.25.

## Context for follow-up

- [The browser tab recording design](../design/design-browser-tab-recording.md) owns the extension's capture. [PRODUCT.md](../../PRODUCT.md) states the 1920x1080 bound under the picture of a recording without saying whether it covers tab recordings.
- The extension's tests (`extension/test`) check the order in which a start passes the tab's size, and can check a bound without a browser.
- Tab recordings are copied as the browser wrote them; the app does not re-encode them, so a bound has to be set where the browser captures.


{% transition from="open" to="done" date="2026-11-25" %}
The app sends its largest picture with start and the extension fits the tab into it (INV-009 of the tab recording design). The browser's handling of the size and the fixed extension were run in Chrome on one Windows PC; a meeting tab recorded by the app with the bound was not.
{% /transition %}
