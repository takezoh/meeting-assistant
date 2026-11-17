---
id: design-meeting-detection
kind: design
title: Meeting detection and recording coordination
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: "Decide from a signal snapshot which rule is satisfied and by which process and window."
- id: RESP-002
  statement: "Decide when a meeting starts and ends by applying a start debounce and an end grace period."
- id: RESP-003
  statement: "Turn meeting starts, ends and manual commands into at most one recording, choosing its video target, audio source and file name."
- id: RESP-004
  statement: "Keep a recording alive across recorder failures and window changes while the meeting continues."
invariants:
- id: INV-001
  statement: "At most one recording exists at any time."
  enforcement: test
- id: INV-002
  statement: "In app scope a recording is never widened to the whole display or to all output audio as a fallback."
  enforcement: test
- id: INV-003
  statement: "A window title is required only to start a title-matched meeting; the meeting continues while the microphone stays in use."
  enforcement: test
- id: INV-004
  statement: "A browser process with a connected extension is not detected through microphone and window signals."
  enforcement: test
- id: INV-005
  statement: "A manual recording is not ended by meeting detection."
  enforcement: test
- id: INV-006
  statement: "A recording is reported as started only after the recorder has reported that capture is running."
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: "Engine::tick, which advances detection and recording by one step and returns the state and notices to show."
  - id: PROV-002
    statement: "Commands for manual start and stop and for pausing automatic recording."
  consumes:
  - id: CONS-001
    statement: "SignalSource: microphone-using processes, the process tree, windows, device state, extension links and extension-reported meetings."
  - id: CONS-002
    statement: "Recorder: start, stop, retarget and take_failure."
  - id: CONS-003
    statement: "Clock, OutputFolder and Pause."
  forbidden:
  - id: FORB-001
    statement: "Reading the clock, the file system or an operating system API directly."
variability:
  fixed:
  - id: FIX-001
    statement: "The meaning of the configuration keys start_debounce_seconds, end_grace_seconds, capture_scope and of the rule fields."
  free:
  - id: FREE-001
    statement: "The default rules, the heuristics that choose a window, and the retry timing."
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: "When a recording fails, keep the partial file, raise a notice and start a new recording while the meeting continues."
- id: FAIL-002
  statement: "When signals cannot be read, report the error for that tick and keep the current recording."
trust_boundaries: []
compatibility_policies:
- id: COMPAT-001
  statement: "An existing config.toml keeps its meaning; a key is not repurposed, and an unknown key is rejected at startup."
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
source_paths:
- crates/core/src/rules.rs
- crates/core/src/detector.rs
- crates/core/src/coordinator.rs
- crates/core/src/engine.rs
- crates/core/src/naming.rs
- crates/core/src/config.rs
- crates/core/src/signals.rs
- crates/core/src/recording.rs
summary: Contract of the rules, the start and end decision, and the coordinator that owns the single recording.
---

## Purpose

Decide, from what can be observed about the machine, when the user is in a meeting and what exactly should be recorded for it, independent of how signals are obtained and how recording is performed.

## Responsibilities

`RuleEvaluator` maps a `SignalSnapshot` to the satisfied rule, with the process that uses the microphone resolved to its top-level owner in the process tree and, where a rule asks for it, the window whose title matched. `MeetingDetector` turns a sequence of evaluations into a start, after the condition has held for the debounce, and an end, after microphone use has been absent for the grace period. `RecordingCoordinator` owns the single recording: it builds the `RecordingRequest`, starts and stops the recorder, moves the video to another window of the app when the recorded one disappears, and restarts after a failure. `Engine` runs these in order once per tick.

## Boundaries

All of this is in `crates/core` and reaches the world only through the ports listed in the front matter. Time arrives as a value from `Clock`. The coordinator does not know whether a `VideoTarget::BrowserTab` is recorded by a browser or a window by the operating system; that is the recorder's concern.

## Invariants

The scope rule (INV-002) is the one most easily broken by a well-meant fallback. When no window of the app is found the request carries `VideoTarget::Blank`; when the meeting has no known audio process and the scope is app, no device-wide audio is requested on its behalf. Only `capture_scope = "screen"` and a manual recording without a detected meeting request the display and the output device.

Extension-reported meetings take precedence for the browser they come from (INV-004), so one call is not recorded both as a tab and as a window.

## Collaboration

`worker` calls `Engine::tick` at the poll interval and forwards commands from the tray. `LinkedSignalSource` and `LinkedRecorder` in the app present the operating system and the extension as one signal source and one recorder.

## Failure Responsibility

A failed start is retried on later ticks while the meeting lasts, with a notice the first time. A failure during recording is collected through `take_failure`, after which the coordinator starts a new file. Stopping always reaches `Recorder::stop`, including when the engine is dropped.

## Variability

Rules are user data. The window heuristic (the title-matched window, otherwise the largest window of the app) and the retry timing may change without changing this contract. The meaning of configuration keys is fixed.

## Conformance

`crates/core/tests/{rules,detector,coordinator,engine}.rs` run the whole component against `crates/core/src/fakes.rs`. Whether the default rules match real meeting apps is not something these tests can establish; see the open issue on verification against real services.

## Related Decisions

[ADR: record only the meeting app by default](../adr/adr-20261117-record-only-the-meeting-app.md). User-facing behavior is stated in `PRODUCT.md`, and the keys in `docs/usage/configuration.md`.
