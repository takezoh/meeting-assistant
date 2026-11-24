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
- id: RESP-004
  statement: Decide where a captured picture of another size goes on the canvas.
invariants:
- id: INV-001
  statement: Every video is written at the configured video.width by video.height, even
    in both dimensions, at least 320 by 240 and at most 1920 by 1080 (the default), whatever
    the target's size and however it changes; each picture is fitted into it with its
    aspect ratio kept.
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
    not replaced by a wider source. The start reports it, with the reason, as a sound
    the recording does not have.
  enforcement: test
- id: INV-005
  statement: A window recording starts with a black picture when no picture arrives
    in time; a display recording fails to start instead.
  enforcement: test
- id: INV-006
  statement: start returns only after the pipeline has opened or has failed to open.
  enforcement: test
- id: INV-007
  statement: Sound captured before the write loop starts is dropped, apart from what
    the first mix needs to start, so audio does not start behind the picture.
  enforcement: test
- id: INV-009
  statement: The sound of a frame interval is written at the next step, once it has
    been captured, and the last interval's sound is written when the recording stops.
  enforcement: test
- id: INV-010
  statement: Each audio input holds the amount queued before a mix near its target
    by dropping or repeating single frames. Silence stands in for an input only before
    it has queued enough to start and after it has run out.
  enforcement: test
- id: INV-008
  statement: A picture at most one pixel larger than the canvas in each direction
    is cropped, not scaled.
  enforcement: test
- id: INV-011
  statement: A meeting or screen recording has exactly two audio tracks, the other
    side first and the microphone second, each written only from its own source; a
    track whose source is turned off, left out by the request, cannot be opened or fails
    holds silence. A recording
    of the microphone alone has no video and one audio track.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: ThreadedRecorder, an implementation of the core Recorder port.
  consumes:
  - id: CONS-001
    statement: 'CaptureBackend: open video, audio and writer (with or without video,
      with the listed audio tracks); take, scale and write pictures; start the audio
      sources, fill a block from each and write it to its track; finalize.'
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
    picture. Of several switch requests queued at once only the last is opened.
trust_boundaries: []
compatibility_policies: []
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
source_paths:
- crates/app/src/recording/plan.rs
- crates/app/src/recording/fit.rs
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

`plan` maps a `RecordingRequest` and the configuration to a `CapturePlan`: the video (a target, a black start, or none) and the two tracks, `OtherSide` (app audio, or output device audio) and `Microphone`, each with its source or none. It also gives the plan for the microphone alone. `fit` decides where a picture of another size goes on the canvas, for a backend that scales pictures itself. `timeline` computes frame and audio timestamps and how long to wait. `mixer` keeps each input's sound queued, as little as mixing allows. `pipeline` is the procedure: open the sources and the writer, wait for the first picture, then per frame interval write the current picture and, for each track, the block of the interval before from its source or of silence, apply a retarget request, and finalize when told to stop. `session` runs a pipeline on a dedicated thread and implements `Recorder` and `MicrophoneRecorder`.

## Boundaries

`CaptureBackend` is the only way the pipeline reaches capture and encoding. It is created inside the recorder thread because capture objects are bound to the thread that creates them. `VideoTarget::BrowserTab` is rejected by the plan: the tab itself is recorded by the extension. The microphone beside a tab recording is recorded by this component, on the microphone-only plan, for the [browser tab recording](design-browser-tab-recording.md).

## Invariants

The canvas size never changes during a recording because the encoder cannot change it, and it does not depend on the target: a window can change shape while it is recorded (a Slack huddle window turned from portrait to landscape as the huddle was joined, and a canvas taken from its first size left about 60% of the later picture black). Every picture, of a resized or different window or of a display, is fitted into the configured size (1920x1080 by default) with its aspect ratio kept; `CaptureBackend::open_video` is given that size. On Windows `fit` scales a larger picture down and puts a smaller one in the middle at its own size, since a recording for minutes gains nothing from enlarging; a window of odd width or height is one pixel larger than its even canvas, and that pixel is cut off instead of scaling the whole picture (INV-008). On macOS ScreenCaptureKit fits the window to the canvas itself. Recordings are for minutes, so the configuration accepts nothing above 1080p. When no new picture arrives, the previous one is written again so the video keeps pace with the audio.

Audio inputs start capturing before the writer is created and the first picture is awaited. When the write loop starts, each input drops what it captured until then, keeping only what the first mix needs, so that the first sound written belongs with the first picture (INV-007).

Whatever is queued in an input when it is mixed is how far its sound lags the picture. The mixer therefore keeps as little queued as it can without running short. The lowest amount queued before a mix is held at one mix, a margin of 20 ms and what a look at mix times alone can miss (the input's largest recent packet or one mix, whichever is smaller); mixing starts, and starts again after an input ran out, with one such packet more, because it may start at any moment of the packet rhythm. A packet counts for two seconds of mixing and at most as 200 ms. The capture clock and the writing clock drift apart; the lowest amount queued is looked at once a second and moved back towards its target by dropping or repeating single frames, which cannot be heard, and an excess above 200 ms (the writing side stalled) is dropped at once (INV-010). When writing falls so far behind that the timeline jumps, the sound queued meanwhile is dropped as at the start, since it belongs to the time jumped over. The sound of an interval is mixed at the next step, when it has arrived, rather than at the start of its own interval, which would take it from what was captured a frame earlier (INV-009).

INV-004 and INV-005 implement the product's scope rule on this side: degraded input gives less in the file, never more than was asked for. INV-011 fixes the layout of the file so that analysis can rely on the second audio track being the user: a missing source makes its track silent rather than removing it or moving the other one into its place.

## Collaboration

`RecordingCoordinator` drives `ThreadedRecorder` through `Recorder`. `platform::recorder` supplies the backend constructor. On Windows the backend uses Windows.Graphics.Capture, the Direct3D 11 video processor, WASAPI process loopback and capture, and the Media Foundation sink writer producing fragmented MP4. The video processor turns each new picture into NV12 (BT.709, limited range) at the canvas size in one pass, and the sink writer chooses the H.264 encoder: the GPU's hardware encoder of whichever vendor, or the software encoder. A GPU whose video processor cannot write NV12 hands over BGRA, which the sink writer converts. Capture is asked for no more pictures than are written (Windows 11 24H2 and later honor it). The pictures are drawn into a few textures; one is drawn into again only once nothing holds the picture in it, neither the pipeline, which writes it again until a new one arrives, nor the encoder, which reads it after it was handed over and lets go of it through a tracked sample. Each audio input's thread sleeps until WASAPI signals that sound arrived. On macOS ScreenCaptureKit is likewise asked for no more pictures than are written.

## Failure Responsibility

`start` waits up to 20 seconds for the pipeline to open and returns its error. A start that did not open in time is told to stop, but its thread keeps preparing until the call it waits in returns, and it may create and finalize the file after `start` returned. After that, a failure ends the thread; the coordinator learns of it through `take_failure` exactly once and may start again. A panic in the thread is reported as a failure, not propagated.

## Variability

A backend may choose any capture API, scaler and encoder that satisfies the port. The PCM format between backend and pipeline is fixed so that the mixer stays platform-independent.

## Conformance

Unit tests in `recording/{plan,fit,timeline,mixer,pipeline,session}.rs` use `pipeline::fakes::FakeBackend`, whose microphone adds a different level than other sources so that each track's content is checked, or no port at all. On macOS the writer tests in `platform/mac/recorder` finish files with two audio tracks and with sound alone, and parse their tracks. They do not exercise the Windows backend. `meeting-assistant --self-test` does, on the machine where it runs, for a display; ignored tests in `platform/win/recorder/{mod,audio}.rs` capture a real window of odd size, resize it and write both pictures; encode NV12 pictures with the GPU's encoder and with the software encoder; decode a file to show that no texture was drawn into while the encoder held it; count the pictures a capture sends at two update intervals; and capture a tone from the output device and from the process playing it. They are run by hand with `cargo test -p meeting-assistant -- --ignored`.

## Related Decisions

[ADR: record only the meeting app by default](../adr/adr-20261117-record-only-the-meeting-app.md) and the [effect boundaries design](design-effect-boundaries.md).
