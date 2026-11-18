---
id: issue-20261118-each-new-picture-gets-a-new-texture
kind: issue
title: Each new picture is copied into a newly created texture
status: open
created: '2026-11-18'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-measured}
- {type: references, target: note-20261118-windows-recording-performance-handoff}
subject_paths:
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/recorder/writer.rs
summary: The Windows scaler creates a canvas-sized texture per new picture because
  the encoder may still read the previous one. Reusing textures needs to know when
  the encoder releases them; measured cost on one PC was not noticeable.
---

## Opportunity

`Scaler::fit` in `crates/app/src/platform/win/recorder/scale.rs` creates a texture at the canvas size for every new picture, and for a picture that is scaled also a render target view and two video processor views. A texture handed to the Media Foundation sink writer may still be read by the encoder after `WriteSample` returns, and the same picture is written again while no new one arrives, so a texture cannot simply be reused for the next picture.

## Evidence

On one PC on 2026-11-18 (NVIDIA hardware encoder, 3840x2160 at 30 fps) a ten-minute self-test used 1.8% of one CPU core, GPU use did not differ from running the stimulus alone, and the encoder took ARGB32 textures directly without a conversion transform ([the measurement note](../note/note-20261118-windows-recording-measured.md)). Removing the allocations would have no visible effect there. A PC whose encoder does not take ARGB32, or that has no hardware encoder, was not measured; the log now names the transforms the sink writer chose.

## Context for follow-up

Step 5 of [the performance handoff](../note/note-20261118-windows-recording-performance-handoff.md). Knowing when a texture is free needs either tracked samples (`IMFTrackedSample`) or the video sample allocator (`MFCreateVideoSampleAllocatorEx`), which must also cover a picture written several times. The [capture pipeline design](../design/design-capture-pipeline.md) leaves the scaling method and the encoder to the backend.
