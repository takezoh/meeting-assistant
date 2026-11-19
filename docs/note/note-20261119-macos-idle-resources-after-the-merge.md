---
id: note-20261119-macos-idle-resources-after-the-merge
kind: note
title: macOS idle resources after merging the Windows work
status: draft
created: '2026-11-19'
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/engine.rs
- crates/app/src/platform/mac/signals.rs
summary: Idle CPU of the merged build on one Mac on 2026-11-19, against the installed
  build that read every signal on every tick.
---

## Summary

On one Mac (macOS 26) on 2026-11-19, with no meeting and `poll_interval_milliseconds = 1000`, the app used 0.62% of one core while it read every signal on every tick, and 0.27% after the merge with the Windows work, which reads only the microphone sessions and the extension's state until some process uses the microphone. One sample of 60 seconds each; the difference is the order of magnitude to expect, not a precise figure.

## Notes

### What was compared

| Build | Signals read while idle | CPU over 60 s | Share of one core | Resident memory |
| --- | --- | --- | --- | --- |
| Installed copy, built at 13:31 from the code of ee9ea9c, running for 25 minutes | processes, windows and every audio process's input and output on every tick | 0.37 s | 0.62% | 61 MiB |
| Release build of the merged tree, started 15 seconds before the sample | the microphone sessions only (one property read per Core Audio process object) and the extension's state | 0.16 s | 0.27% | 72 MiB |

Both ran with the browser extension connected and the status panel closed. The merged build was started from the terminal with its own `--config`, after the installed copy had been stopped with `--quit`; the installed copy was started again afterwards. Memory was not compared on equal terms: the installed copy had run for 25 minutes and the merged build for 15 seconds, and the installed copy predates the panel's web view being let go.

Reading the on-screen window list with Core Graphics took 1.5 ms per read with 32 windows on screen (200 reads timed by a separate script). The process list was not timed on its own.

### The macOS parts of the Windows optimizations

The Windows work made three changes that have a macOS side:

| Windows change | macOS state |
| --- | --- |
| Capture asked for no more pictures than are written | ScreenCaptureKit's `minimumFrameInterval` was already the frame interval |
| NV12 handed to the encoder | ScreenCaptureKit already delivered `420v`, which the H.264 encoder takes without converting |
| The panel's web view let go 30 seconds after the panel closes | shared code in `tray.rs`, so macOS does it too; the web view's memory level is a WebView2 setting with no macOS counterpart |

The selective reading of signals is in `crates/core/src/engine.rs`, shared by both platforms. On macOS the microphone sessions now read only whether each audio process is taking input, and look up its device only for a process that is; before, each tick read both directions of every audio process and the devices of both.

### Not measured

- CPU and memory while recording on macOS, before or after the merge.
- How often the windows are read during a day: they are read while any process uses the microphone, not only a meeting app.
