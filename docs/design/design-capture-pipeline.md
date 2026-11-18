---
id: design-capture-pipeline
kind: design
title: Capture pipeline
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: Decide from a recording request and the configuration which video and
    audio sources to open and the output format.
- id: RESP-002
  statement: 'Run one recording: open sources and the writer, obtain the first picture,
    write video and audio on a common timeline, switch windows on request, finalize.'
- id: RESP-003
  statement: Present a pipeline running on its own thread as a core Recorder.
invariants:
- id: INV-001
  statement: The output size is fixed when the recording opens, is even in both dimensions
    and is at least the encoder minimum.
  enforcement: test
- id: INV-002
  statement: Video and audio timestamps derive from the same elapsed time.
  enforcement: test
- id: INV-003
  statement: A recording that opened its writer finalizes it on stop, on failure and
    when the recorder is dropped.
  enforcement: test
- id: INV-004
  statement: An audio source that cannot be opened is skipped and is not replaced
    by a wider source.
  enforcement: test
- id: INV-005
  statement: A window recording starts with a black picture when no picture arrives
    in time; a display recording fails to start instead.
  enforcement: test
- id: INV-006
  statement: start returns only after the pipeline has opened or has failed to open.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: ThreadedRecorder, an implementation of the core Recorder port.
  consumes:
  - id: CONS-001
    statement: 'CaptureBackend: open video, audio and writer; take, scale and write
      pictures; mix and write audio; finalize.'
  - id: CONS-002
    statement: Pacer for elapsed time and sleeping, and Control for stop and retarget
      requests.
  forbidden:
  - id: FORB-001
    statement: Calling an operating system API, reading a clock or spawning a thread
      from plan, timeline, mixer or pipeline.
variability:
  fixed:
  - id: FIX-001
    statement: Audio is handled as 48 kHz, two-channel, 16-bit PCM between the backend
      and the pipeline.
  free:
  - id: FREE-001
    statement: The container, codecs, scaling method and capture APIs of a backend.
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: A failure while opening is returned from start; a failure while running
    ends the thread and is delivered once through take_failure.
- id: FAIL-002
  statement: A failed switch to another window leaves the recording on its current
    picture.
trust_boundaries: []
compatibility_policies: []
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
source_paths:
- crates/app/src/recording/plan.rs
- crates/app/src/recording/timeline.rs
- crates/app/src/recording/mixer.rs
- crates/app/src/recording/pipeline.rs
- crates/app/src/recording/session.rs
- crates/app/src/platform/win/recorder
- crates/app/src/recording/pcm.rs
- crates/app/src/platform/mac/recorder
summary: Contract of one OS-path recording, from the plan through the write loop to
  finalization, and of the CaptureBackend port.
---

## Purpose

Produce one playable file from a recording request, with the procedure shared across platforms and only the capture and encode operations supplied per platform.

## Responsibilities

`plan` maps a `RecordingRequest` and the configuration to a `VideoSource`, a list of `AudioSource` and a `MediaFormat`. `timeline` computes frame and audio timestamps and how long to wait. `mixer` adds PCM inputs with saturation. `pipeline` is the procedure: open the sources and the writer, wait for the first picture, then per frame interval write the current picture and the matching block of mixed audio, apply a retarget request, and finalize when told to stop. `session` runs a pipeline on a dedicated thread and implements `Recorder`.

## Boundaries

`CaptureBackend` is the only way the pipeline reaches capture and encoding. It is created inside the recorder thread because capture objects are bound to the thread that creates them. `VideoTarget::BrowserTab` is rejected by the plan; tab recordings do not use this component.

## Invariants

The canvas size never changes during a recording because the encoder cannot change it; a resized or different window is scaled into the canvas with its aspect ratio kept. When no new picture arrives, the previous one is written again so the video keeps pace with the audio.

INV-004 and INV-005 implement the product's scope rule on this side: degraded input gives less in the file, never more than was asked for.

## Collaboration

`RecordingCoordinator` drives `ThreadedRecorder` through `Recorder`. `platform::recorder` supplies the backend constructor. On Windows the backend uses Windows.Graphics.Capture, the Direct3D 11 video processor, WASAPI process loopback and capture, and the Media Foundation sink writer producing fragmented MP4.

## Failure Responsibility

`start` waits up to 20 seconds for the pipeline to open and returns its error. After that, a failure ends the thread; the coordinator learns of it through `take_failure` exactly once and may start again. A panic in the thread is reported as a failure, not propagated.

## Variability

A backend may choose any capture API, scaler and encoder that satisfies the port. The PCM format between backend and pipeline is fixed so that the mixer stays platform-independent.

## Conformance

Unit tests in `recording/{plan,timeline,mixer,pipeline,session}.rs` use `pipeline::fakes::FakeBackend`. They do not exercise the Windows backend; `meeting-assistant --self-test` does, on the machine where it runs.

## Related Decisions

[ADR: record only the meeting app by default](../adr/adr-20261117-record-only-the-meeting-app.md) and the [effect boundaries design](design-effect-boundaries.md).
