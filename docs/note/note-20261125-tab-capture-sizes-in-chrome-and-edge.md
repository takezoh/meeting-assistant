---
id: note-20261125-tab-capture-sizes-in-chrome-and-edge
kind: note
title: Tab capture sizes in Chrome and Edge, 2026-11-25
status: draft
created: '2026-11-25'
tags: []
owners: []
relations:
- {type: references, target: issue-20261125-tab-recordings-are-taken-above-1080p}
source_paths:
- extension/background.js
- extension/offscreen.js
summary: In Chrome 158 and Edge 158 on Windows, a tab captured with only maxWidth
  and maxHeight was delivered and recorded at exactly that size for the whole capture,
  with the tab fitted into it and black bars; a tab smaller than the size was scaled
  up.
---

## Summary

On 2026-11-25, on one Windows 11 PC (10.0.26200) with a 3840x2160 display at 125 %, a test tab was captured in Chrome 158.0.8127.38 and Edge 158.0.4370.33 with the constraints of `extension/offscreen.js` and recorded with its `MediaRecorder` options. Both browsers behaved the same:

- **The maximum is the size.** With only `maxWidth` and `maxHeight`, every picture came at that size, whether the tab was larger, smaller or of another shape. Equal minimum and maximum gave the same pictures. With no size at all, pictures came at the display's 3840x2160.
- **The tab keeps its shape.** It was scaled to fit the pictures, centered, with black bars where the shapes differ, and scaled up when it was smaller.
- **The size holds for the whole capture.** When the window was resized during a recording, the pictures kept their first size and the tab was fitted into them again. The MP4 kept one sample description at that size, and `MediaRecorder` reported no error.
- **The current limit reproduces the issue.** The tab's size times the device pixel ratio, as `extension/background.js` passes it, gave pictures of 2232x1446 in Chrome and 2222x1450 in Edge.

A size computed as the tab's size in device pixels, scaled down into 1920x1080 with its shape kept and never up, gave pictures of exactly that size, filled by the tab. After the fix, the repository's extension computed such a size from the bound in `start` and recorded a test tab at it in Chrome, without the app.

## Notes

### How it was run

The extension in the repository was not used, to keep the installed app out of it. A probe extension in a scratch folder did what the start of a recording does:

- It read the tab's width and height with `chrome.tabs.get` and multiplied them by the page's `devicePixelRatio`, as `startRecording` does with the ratio `bridge.js` reports. It then asked for the tab's stream ID.
- Its offscreen document called `getUserMedia` with `chromeMediaSource: "tab"`, `maxFrameRate: 30` and the size under test. It recorded with the same MIME types, bit rates and key frame interval as `offscreen.js`.

Each browser ran with a throwaway profile. The probe was loaded over the DevTools pipe (`--remote-debugging-pipe`, `--enable-unsafe-extension-debugging`, `Extensions.loadUnpacked`), and `--allowlisted-extension-id` let it capture without a user gesture. The same script drove both browsers.

The tab showed a blue page with an 8-pixel red border and moving content. For each capture:

- **Sizes:** the coded, visible and display size of every picture, read with `MediaStreamTrackProcessor` from a clone of the video track.
- **Placement:** once a second, the extent of non-black pixels through the middle row and column of a picture. The red border was found at each edge of that extent, so the whole page was in the picture.
- **The file:** the track header, the sample description, its sequence parameter set, and any parameter sets in the samples.

Chrome ran from 21:15 to 21:17 and Edge from 21:18 to 21:20, local time.

### Sizes

The large window's tab was 1786x1157 in Chrome and 1778x1160 in Edge, which is 2233x1446 and 2223x1450 in device pixels. The small window's tab was 886x606 and 878x609, which is 1108x758 and 1098x761.

| Tab | Size passed | Chrome: pictures and file | Chrome: the tab in them | Edge: pictures and file | Edge: the tab in them |
| --- | --- | --- | --- | --- | --- |
| Large | Its size in device pixels (current) | 2232x1446 | Whole picture | 2222x1450 | Whole picture |
| Large | Fitted into 1920x1080 | 1668x1080 | Whole picture | 1656x1080 | Whole picture |
| Large | Maximum 1920x1080 | 1920x1080 | 1666 wide, bars of 126 each side | 1920x1080 | 1656 wide, bars of 132 each side |
| Large | Minimum and maximum 1920x1080 | 1920x1080 | As with the maximum alone | 1920x1080 | As with the maximum alone |
| Large | None | 3840x2160 | 3334 wide, scaled up | 3840x2160 | 3312 wide, scaled up |
| Small | Its size in device pixels, which fitted already | 1108x758 | Whole picture | 1098x760 | Whole picture |
| Small | Maximum 1920x1080 | 1920x1080 | 1578x1080, scaled up | 1920x1080 | 1558x1080, scaled up |
| Small | Minimum and maximum 1920x1080 | 1920x1080 | As with the maximum alone | 1920x1080 | As with the maximum alone |

"Whole picture" allows up to 2 black pixels at one edge. That comes from rounding: the passed size differs by a pixel or so from the tab's real one, and the pictures have even dimensions.

### Resizing during a recording

Each of these recordings ran for 15 seconds:

- 0 s: the large window.
- 5 s: the window was made smaller. The tab became 1186x706 in Chrome and 1178x710 in Edge.
- 10 s: the window was made larger than at the start. The tab became 2386x1406 and 2378x1410.

| Size passed | Pictures | The tab's rows in them: Chrome | The tab's rows in them: Edge |
| --- | --- | --- | --- |
| Fitted into 1920x1080 (1668x1080 and 1656x1080) | The first size throughout | All, then 42-1035, then 48-1029 | All, then 40-1037, then 48-1029 |
| Its size in device pixels (2232x1446 and 2222x1450) | The first size throughout | All, then 58-1385, then 64-1379 | All, then 56-1393, then 66-1381 |

After the window became smaller, the tab was scaled up to the picture's width. In Chrome's fitted case it went from about 1482x882 device pixels to 1668x994. After the window became larger, the tab was scaled down. Each file had:

- one `avc1` sample description at the first size;
- no parameter sets inside the samples;
- 453 to 455 video samples.

### The repository's extension after the fix

From 21:55 to 21:57 local time, a copy of the repository's `extension` folder ran in Chrome 158 with a throwaway profile. It carried the probe's key, so the native host, registered for the real extension ID only, refused it, and the installed app was never contacted. Three steps stood in for the app and the meeting page:

- **The pixel ratio:** set in `pixelRatios`, as `bridge.js` reports it from a meeting page.
- **The start:** the app's `start` was given to `onAppMessage` in the service worker, with `max_size`.
- **The upload:** a small WebSocket receiver in the scratch folder took the recording.

After five seconds a `stop` was given the same way. The offscreen document closed the upload with code 1000.

| Tab (CSS pixels at 1.25) | `max_size` | Recorded |
| --- | --- | --- |
| 1786x1157 | 1920x1080 | 1668x1080 |
| 1786x1157 | 1280x720 | 1112x720 |
| 886x606 | 1920x1080 | 1108x758 |

Each file had one `avc1` sample description at that size.

### A zoomed tab, 2026-11-27

At 01:52 local time on 2026-11-27 the same setup ran again in Chrome 158 with the small window. The tab was zoomed to 150 % with `chrome.tabs.setZoom`, and the extension had been changed to take the tab's zoom out of the page's ratio:

- The page's `innerWidth` and `innerHeight` became 591x404 and its `devicePixelRatio` 1.875. `chrome.tabs.get` still gave 886x606.
- `chrome.tabs.getZoom` gave 1.5 to the extension, which has no `tabs` permission.
- Given the page's ratio, the extension kept 1.25 device pixels per pixel of the tab's size. With a `max_size` of 1920x1080 it recorded the tab at 1108x758, as at 100 %.

Multiplying the tab's size by the page's ratio, as the extension did before, gives 1661x1136 for this tab, which fitted into 1920x1080 is 1579x1080: the tab would have been scaled up. That is computed, not recorded.

### Not established here

- The app's `start` over native messaging, a meeting page, and the app's copy of the recording.
- The repository's extension in Edge.
- macOS, a device pixel ratio other than 1.25, and other browser versions.
- How many times the browser scales a picture inside its capture. Only the pictures it delivered were observed.
