---
id: note-20261118-windows-recording-performance-handoff
kind: note
title: Windows recording performance handoff
status: draft
created: '2026-11-18'
topic: performance
summary: Handoff for improving the Windows capture and encode path (A/V offset, encoder input format, per-frame
  GPU allocations, odd sizes, canvas cap). Found by reading the source on a Mac; nothing was run on Windows.
tags: []
owners: []
relations:
- {type: references, target: design-capture-pipeline}
source_paths:
- crates/app/src/recording/pipeline.rs
- crates/app/src/recording/mixer.rs
- crates/app/src/recording/timeline.rs
- crates/app/src/recording/plan.rs
- crates/app/src/platform/win/recorder/audio.rs
- crates/app/src/platform/win/recorder/video.rs
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/recorder/writer.rs
---

## Summary

A pre-design handoff for the Windows recording path, written on 2026-11-18 after a read-only review on a Mac. The video path is sound in outline: capture, scaling and the encoder share one Direct3D 11 device, frames reach the sink writer as DXGI surfaces without a CPU readback, and pacing follows an absolute schedule. The problems below are an audio offset at the start, an unverified encoder choice with a colour conversion left to Media Foundation, and per-frame GPU allocations. None of it was run or measured on Windows; numbers are estimates for 1920x1080 at 30 fps.

Since commit ee9ea9c (2026-11-19) the canvas is fixed at the configured size, 1920x1080 by default: F7, F8, I5 and steps 3 and 7 no longer apply, and F6 applies to almost every frame. See [the Windows handoff of 2026-11-19](note-20261119-windows-handoff-after-the-macos-work-of-.md), which lists the other changes to check on Windows.

## Notes

### Purpose

Keep audio and video aligned for the length of a recording, keep encoding on the GPU, and stop doing per-frame work that can be done once, without changing what is recorded.

### Facts read in the source (2026-11-18, commit d821c3b)

| # | Fact | Where |
| --- | --- | --- |
| F1 | Audio capture starts when an input is opened (`IAudioClient::Start`), before the writer is created and before the first picture is awaited (up to 2 s) | `pipeline.rs` open: audio loop, then `open_writer`, then `first_picture`; `audio.rs` `open_client` |
| F2 | The mixer primes at 40 ms and only trims a backlog above 500 ms, back down to 40 ms; an underrun inserts silence and re-primes | `mixer.rs` `BufferPolicy::for_format`, `SourceBuffer::mix_into` |
| F3 | Audio packets are timed by arrival, not by device position: `GetBuffer` is called without the QPC position | `audio.rs` around the `GetBuffer` call |
| F4 | The video input type is `MFVideoFormat_ARGB32`; output is H.264 Main; hardware transforms and the DXGI device manager are enabled; throttling is disabled; the selected encoder is not logged | `writer.rs` `create`, `add_video_stream` |
| F5 | Every new picture allocates a texture and copies the captured surface into it | `video.rs` `copy_frame` |
| F6 | When the picture size differs from the canvas, each frame also allocates a black texture with a render target view and clears it, and creates the video processor's input and output views | `scale.rs` `fit`, `black`, `Processor::blt` |
| F7 | The canvas is the source size rounded down to even numbers, so a window with an odd width or height always takes the scaling path of F6 to shrink by one pixel | `plan.rs` `canvas_size`, `scale.rs` `fit` |
| F8 | The canvas has no upper bound | `plan.rs` `canvas_size` |
| F9 | When writing falls more than 2 s behind, the timeline jumps and neither video nor audio is written for the gap | `pipeline.rs` `RESYNC_THRESHOLD`, `timeline.rs` `skip_ahead_if_late` |
| F10 | Every queued retarget request opens a capture in the write loop, one after another | `pipeline.rs` `run`, `switch_window` |

### Inferences that need a Windows machine

- I1 (from F1, F2): audio lags video by the backlog collected before the first frame, up to 0.5 s, for the whole file. The app audio and the microphone are offset by the time between their opens.
- I2 (from F2, F3): the audio device clock and the QPC clock drift. Surplus is dropped only at 500 ms, all at once; a shortfall produces about 40 ms of silence each time.
- I3 (from F4): the sink writer inserts a video processor MFT to turn ARGB32 into NV12. With a hardware encoder that stays on the GPU; with the software encoder each frame is read back to system memory and encoded on the write-loop thread, which can reach F9 and drop seconds at a time. Which one is chosen is not known.
- I4 (from F5, F6): about 0.5 to 1 GB/s of GPU memory traffic and two to six resource creations per frame that are not needed.
- I5 (from F8): a 4K display is encoded at 4K, and widths over 4096 exceed many hardware H.264 encoders.

### Agreed direction

Leave Windows work for a session that can run on Windows; do not change it blind. Do not add fallbacks for old systems.

### Proposed steps, cheapest first

1. Measure before changing: log the encoder MFT chosen after `BeginWriting` (`IMFSinkWriterEx::GetTransformForStream`, and whether it is hardware) and the sink writer statistics at finalize. Record a 10-minute and a 60-minute session and measure the A/V offset at start and end.
2. Start offset: when the write loop starts, cut each audio buffer down to the prime amount, and lower the catch-up limit from 500 ms to around 120 ms. The deciding code is in `mixer.rs` and `pipeline.rs` and can be tested with fakes.
3. Odd sizes: crop by up to one pixel instead of scaling.
4. Retarget: apply only the last queued request.
5. GPU path: one video processor blit per new picture from the capture surface into a reused NV12 texture, with scaling and BGRA-to-NV12 (BT.709, limited range) in the same pass, and the input type set to NV12. Cache the processor views. Reuse output textures from a ring deeper than the encoder's queue, or let `MFCreateVideoSampleAllocatorEx` manage them, so a texture is never overwritten while the encoder still holds it.
6. Drift: place audio packets on the timeline by their device QPC position, and correct by dropping or repeating single samples, or by a slight resample.
7. Canvas cap: decide as a product question whether to bound the long side.

### Verification

Each step is accepted on Windows by the measurements of step 1, not by unit tests alone: the offset at start under one frame, the offset after 60 minutes within a frame, the encoder reported as hardware, no F9 skips in the log, and GPU and CPU load compared before and after.

### Starting a session

Read this note, then [the capture pipeline design](../design/design-capture-pipeline.md), then the files in `source_paths`. Run `meeting-assistant --self-test` on Windows to get a baseline file before changing anything. Keep the boundaries in AGENTS.md: deciding code stays in `recording/`, OS calls stay under `platform/win`.

### Not settled

- Whether hardware encoders on the target machines accept ARGB input directly, which would make the NV12 step smaller.
- The exact drift between the audio clock and QPC on real devices.
- Whether to cap the canvas size.
