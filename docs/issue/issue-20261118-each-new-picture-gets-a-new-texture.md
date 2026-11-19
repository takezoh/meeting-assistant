---
id: issue-20261118-each-new-picture-gets-a-new-texture
kind: issue
title: Each new picture is copied into a newly created texture
status: done
created: '2026-11-18'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-measured}
subject_paths:
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/recorder/writer.rs
summary: The Windows scaler creates a canvas-sized texture per new picture because
  the encoder may still read the previous one. Reusing textures needs to know when
  the encoder releases them; measured cost on one PC was not noticeable.
disposition_target: note-20261119-windows-idle-and-recording-resources
updated: '2026-11-19'
---

## Opportunity

`Scaler::fit` in `crates/app/src/platform/win/recorder/scale.rs` creates an NV12 texture at the canvas size and two video processor views for every new picture (since 2026-11-19; before, a BGRA texture, and for a scaled picture a render target view too). A texture handed to the Media Foundation sink writer may still be read by the encoder after `WriteSample` returns, and the same picture is written again while no new one arrives, so a texture cannot simply be reused for the next picture.

## Evidence

On one PC on 2026-11-18 (NVIDIA hardware encoder, 3840x2160 at 30 fps) a ten-minute self-test used 1.8% of one CPU core, GPU use did not differ from running the stimulus alone, and the encoder took ARGB32 textures directly without a conversion transform ([the measurement note](../note/note-20261118-windows-recording-measured.md)). Removing the allocations would have no visible effect there. A PC whose encoder does not take ARGB32, or that has no hardware encoder, was not measured; the log now names the transforms the sink writer chose.

## Resolution

Since 2026-11-19 the scaler draws pictures into a pool of textures and draws into one again only once neither the pipeline nor the encoder holds it; the sink writer is given tracked samples, which report when the encoder lets go. Measured in [the note of 2026-11-19](../note/note-20261119-windows-idle-and-recording-resources.md).

## Context for follow-up

Knowing when a texture is free needs either tracked samples (`IMFTrackedSample`) or the video sample allocator (`MFCreateVideoSampleAllocatorEx`), which must also cover a picture written several times. The [capture pipeline design](../design/design-capture-pipeline.md) leaves the scaling method and the encoder to the backend.
