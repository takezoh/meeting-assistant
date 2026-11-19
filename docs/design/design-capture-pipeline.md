---
id: design-capture-pipeline
kind: design
title: Capture pipeline
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: Decide from a recording request and the configuration the video source,
    the source of each of the two audio tracks and the output format; and the plan
    for recording the microphone alone.
- id: RESP-002
  statement: 'Run one recording: open sources and the writer, obtain the first picture,
    write video and each audio track on a common timeline, switch windows on request,
    finalize.'
- id: RESP-003
  statement: Present a pipeline running on its own thread as a core Recorder, and
    as the MicrophoneRecorder that records the microphone beside a browser tab recording.
invariants:
- id: INV-001
  statement: Every video is written at the configured video.width by video.height (1920x1080
    by default and at most), whatever the target's size and however it changes; each
    picture is fitted into it with its aspect ratio kept.
  enforcement: test
- id: INV-002
  statement: Video and audio timestamps derive from the same elapsed time.
  enforcement: test
- id: INV-003
  statement: A recording that opened its writer finalizes it on stop, on failure and
    when the recorder is dropped.
  enforcement: test
- id: INV-004
  statement: An audio source that cannot be opened leaves its track silent and is
    not replaced by a wider source.
  enforcement: test
- id: INV-005
  statement: A window recording starts with a black picture when no picture arrives
    in time; a display recording fails to start instead.
  enforcement: test
- id: INV-006
  statement: start returns only after the pipeline has opened or has failed to open.
  enforcement: test
- id: INV-007
  statement: A meeting or screen recording has exactly two audio tracks, the other
    side first and the microphone second, each written only from its own source; a
    track whose source is turned off, cannot be opened or fails holds silence. A recording
    of the microphone alone has no video and one audio track.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: ThreadedRecorder, an implementation of the core Recorder port.
  consumes:
  - id: CONS-001
    statement: 'CaptureBackend: open video, audio and writer (with or without video,
      with the listed audio tracks); take, scale and write pictures; fill a block from
      an audio source and write it to a track; finalize.'
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

Produce one file from a recording request, with the procedure shared across platforms and only the capture and encode operations supplied per platform. The file keeps the other side and the user apart, in two audio tracks, because recordings are analysed into minutes rather than played.

## Responsibilities

`plan` maps a `RecordingRequest` and the configuration to a `CapturePlan`: the video (a target, a black start, or none) and the two tracks, `OtherSide` (app audio, or output device audio) and `Microphone`, each with its source or none. It also gives the plan for the microphone alone. `timeline` computes frame and audio timestamps and how long to wait. `mixer` buffers each PCM source against jitter and drops what has piled up. `pipeline` is the procedure: open the sources and the writer, wait for the first picture, then per frame interval write the current picture and, for each track, the matching block of its source or of silence, apply a retarget request, and finalize when told to stop. `session` runs a pipeline on a dedicated thread and implements `Recorder` and `MicrophoneRecorder`.

## Boundaries

`CaptureBackend` is the only way the pipeline reaches capture and encoding. It is created inside the recorder thread because capture objects are bound to the thread that creates them. `VideoTarget::BrowserTab` is rejected by the plan: the tab itself is recorded by the extension. The microphone beside a tab recording is recorded by this component, on the microphone-only plan, for the [browser tab recording](design-browser-tab-recording.md).

## Invariants

The canvas size never changes during a recording because the encoder cannot change it, and it does not depend on the target: a window can change shape while it is recorded (a Slack huddle window turned from portrait to landscape as the huddle was joined, and a canvas taken from its first size left about 60% of the later picture black). Every picture, of a resized or different window or of a display, is scaled into the configured size (1920x1080 by default) with its aspect ratio kept; `CaptureBackend::open_video` is given that size. Recordings are for minutes, so the configuration accepts nothing above 1080p. When no new picture arrives, the previous one is written again so the video keeps pace with the audio.

INV-004 and INV-005 implement the product's scope rule on this side: degraded input gives less in the file, never more than was asked for. INV-007 fixes the layout of the file so that analysis can rely on the second audio track being the user: a missing source makes its track silent rather than removing it or moving the other one into its place.

## Collaboration

`RecordingCoordinator` drives `ThreadedRecorder` through `Recorder`. `platform::recorder` supplies the backend constructor. On Windows the backend uses Windows.Graphics.Capture, the Direct3D 11 video processor, WASAPI process loopback and capture, and the Media Foundation sink writer producing fragmented MP4.

## Failure Responsibility

`start` waits up to 20 seconds for the pipeline to open and returns its error. After that, a failure ends the thread; the coordinator learns of it through `take_failure` exactly once and may start again. A panic in the thread is reported as a failure, not propagated.

## Variability

A backend may choose any capture API, scaler and encoder that satisfies the port. The PCM format between backend and pipeline is fixed so that the mixer stays platform-independent.

## Conformance

Unit tests in `recording/{plan,timeline,mixer,pipeline,session}.rs` use `pipeline::fakes::FakeBackend`, whose microphone adds a different level than other sources so that each track's content is checked. On macOS the writer tests in `platform/mac/recorder` finish files with two audio tracks and with sound alone, and parse their tracks. They do not exercise the Windows backend; `meeting-assistant --self-test` does, on the machine where it runs.

## Related Decisions

[ADR: record only the meeting app by default](../adr/adr-20261117-record-only-the-meeting-app.md) and the [effect boundaries design](design-effect-boundaries.md).
