---
id: note-20261118-windows-recording-measured
kind: note
title: Windows recording measured, 2026-11-18
status: draft
created: '2026-11-18'
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-performance-handoff}
- {type: references, target: design-capture-pipeline}
source_paths:
- crates/app/src/recording/mixer.rs
- crates/app/src/recording/pipeline.rs
- crates/app/src/recording/fit.rs
- crates/app/src/platform/win/recorder/writer.rs
- crates/app/src/platform/win/mod.rs
- tools/avsync/src/main.rs
summary: A/V offset, drift, encoder and load of the Windows recording path measured
  on one PC before and after the changes from the performance handoff, and the status
  panel seen on Windows.
topic: verification
---

## Summary

On 2026-11-18 the Windows recording path was measured on one PC, following [the performance handoff](note-20261118-windows-recording-performance-handoff.md), before and after the changes made from it. Sound started about 150 ms behind the picture and stayed there for the whole file; after dropping the backlog it started within a frame. Dropping the backlog alone made the sound run short a few times in ten minutes; the mixer's depth control that followed was checked in simulation, and its long run on this PC had not finished when this note was written. The encoder was the NVIDIA hardware encoder taking ARGB32 directly, and the recording used little CPU, so the handoff's GPU-path work brought nothing measurable on this PC and was left as an issue. The same day the status panel was given the Windows 11 flyout look and was seen on this PC.

These are observations from one machine on one day, over Remote Desktop and without a microphone. They do not establish behavior on other hardware, other encoders or in real calls.

## Notes

### Machine and method

- Windows 11 Pro 10.0.26200, NVIDIA GeForce RTX 4060 Ti, one 3840x2160 display, Rust 1.98.1. The session was a Remote Desktop session: the default output device was the session's, and there was no microphone, so every recording had the output audio only.
- `meeting-assistant --self-test <file> <seconds>` with a development configuration recorded the primary display and the output audio. [`tools/avsync`](../../tools/avsync/README.md) flashed a square and played a tone together every 2 s (4 s in the 60-minute run) and paired them in the file. Its offsets read about 30 ms earlier than the queue the log reported (146 ms measured against 179 ms queued), and single values jump by a frame; medians per minute are given.
- GPU use was sampled once a second with `nvidia-smi`, the app's CPU time read from the process after it exited.

### Before the changes

The build from commit `9a156fd` with only the new log lines.

| Run | Sound behind picture (median) | Course | Log |
| --- | --- | --- | --- |
| 10 min | 146 ms | 146 ms in every minute; trend -1.2 ms/min | 179 ms queued when mixing began; no catch-up, no underrun; 18001 frames, 5 more than a frame late, the slowest 227 ms while a build ran; no skip |
| 2 min | 146 ms | | 1.9 s user and 2.0 s kernel CPU time, 3.3% of one core |

- The sink writer chose `NVIDIA H.264 Encoder MFT`, a hardware transform, and fed it ARGB32 with no conversion transform in front. At stop the encoder had nothing queued.
- GPU use averaged 15% with the recording and 18% with the stimulus alone; the encoder engine was busy 32% of the time. The recording made no difference to GPU use that the samples could show.
- Ten minutes of a mostly still 3840x2160 desktop came to 80 MB.

### After dropping the backlog, cropping odd sizes and opening only the last window switch

| Run | Sound behind picture (median) | Course | Log |
| --- | --- | --- | --- |
| 10 min | 13 ms | 7, 30, 7, 7, 13, 13, 34, 37, 37, 37 ms per minute | 169 ms dropped when writing started, 40 ms queued when mixing began; 4 underruns (23 ms missing); no late frame, the slowest 15 ms; 1.8% of one core; 96 MB |

Keeping only 40 ms against a mix of 33 ms left about 7 ms of margin, and the sound ran short four times; each refill added delay, which is the rise from 7 to 37 ms.

### After the mixer's depth control and writing each interval's sound a step later

A 60-minute run of this change was still recording when this note was written, and its result is not recorded here. The change was checked only by unit tests that simulate 20 minutes of 10 ms and 100 ms packets on a clock up to 200 ppm off, with no underrun. Two fixes from review came after that run started: a packet size is forgotten after two seconds of mixing, and the sound queued while writing jumped over a stall is dropped.

### Window pictures

`window_pictures_are_cropped_when_odd_and_scaled_when_resized_and_written`, run with `--ignored`, passed: a 641x481 window was recorded at 640x480 with its last column and row cut off and not blended in, a resize to 800x300 was scaled by the GPU video processor straight from the capture surface with black margins, and both pictures were written to an MP4. With cropping switched off the test failed.

### Status panel

A development build was run with its own configuration while the installed copy was stopped, and its tray icon was clicked by posting the icon's message to its window; the installed copy was started again afterwards.

| What was done | Observed |
| --- | --- |
| Panel with the transparent window, the transient-window acrylic and the frame tao keeps for a borderless window's shadow | The page was opaque white. Without that frame the acrylic showed |
| Panel as built, light theme, no meeting | Acrylic behind a see-through page, rounded corners, Segoe UI Variable, the accent color on the switch, Windows 11 buttons; it hung above the taskbar, centered on the overflow arrow that held the icon |
| The page with sample content for a recording and for a detected meeting, light and dark, in headless Edge 157.0.4341.60 | The layout and the Windows 11 controls in both themes; the macOS design unchanged |

The WebView2 runtime was 157.0.4341.46.

### Not checked

- Anything with a microphone, app audio from a single process, a real meeting, or a window recorded by the app rather than by the ignored test.
- Another graphics adapter or encoder, a display wider than 4096 pixels ([issue](../issue/issue-20261118-the-recording-canvas-has-no-upper-bound.md)).
- The tray icon itself, its tooltip with the running time, the menu, the panel in the dark theme or with meetings in the real app, and Windows 10.
- The macOS code: `cargo clippy --target aarch64-apple-darwin` could not run on this PC, because a dependency's build script needs a C compiler for that target. The mixer changes also apply to macOS and were only simulated there in unit tests.
