---
id: design-meeting-detection
kind: design
title: Meeting detection and recording coordination
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: Decide from a signal snapshot which rule is satisfied, by which process,
    and which windows its app has.
- id: RESP-002
  statement: Decide when each meeting starts and ends by applying a start debounce
    and an end grace period to that meeting alone, with separate timings for desktop
    apps and for meetings the extension reports; several meetings can be in progress
    at once.
- id: RESP-003
  statement: Turn meeting starts, ends and manual commands into at most one recording,
    choosing which detected meeting it records, its video target, audio source and
    file name; a recording by hand can instead be of the desktop or of a window the
    user chose, with or without the microphone.
- id: RESP-005
  statement: Report the detected meetings, in the order they started, and the recording,
    so that the user can see and act on each meeting.
- id: RESP-004
  statement: Keep a recording alive across recorder failures and window changes while
    the meeting continues.
invariants:
- id: INV-001
  statement: At most one recording exists at any time.
  enforcement: test
- id: INV-002
  statement: In app scope a recording is never widened to the whole display or to
    all output audio as a fallback.
  enforcement: test
- id: INV-003
  statement: A meeting in a browser is detected only from the extension's reports.
    Rules start desktop meetings from microphone use; such a meeting continues while
    its app uses the microphone and ends on that alone, never because of a window.
    The default rules name no browser, and no meeting is detected from a window title;
    an app's adapter may read titles only to tell which of the app's windows is which.
  enforcement: test
- id: INV-004
  statement: A browser process with a connected extension is not detected through
    microphone and window signals.
  enforcement: test
- id: INV-005
  statement: A manual recording is not ended by meeting detection.
  enforcement: test
- id: INV-006
  statement: A recording is reported as started only after the recorder has reported
    that capture is running.
  enforcement: test
- id: INV-007
  statement: A meeting is identified by its rule and, for an extension-reported meeting,
    its tab; one meeting's evidence neither starts nor keeps alive another.
  enforcement: test
- id: INV-008
  statement: A meeting whose recording the user stopped, or whose automatic recording
    gave up, is not recorded automatically again until automatic recording is switched
    on again.
  enforcement: test
- id: INV-009
  statement: A desktop meeting's picture is only a window of its app that the app's
    adapter tells is a meeting's, or, of the windows the adapter tells nothing about,
    one that appeared while the windows were being read, from MEETING_WINDOW_LOOKBACK
    before its microphone use began; it is chosen by the start or later during the
    meeting. A window the adapter tells is the app's main window, and an untold window
    open when reading began, are never recorded; until the meeting has a window its
    picture is black.
  enforcement: test
- id: INV-010
  statement: What is known of a meeting is one MeetingMetadata whatever told it (an
    app's adapter from the meeting's window, or the browser extension), kept by the
    same rules; what is learned during a meeting is reported once and kept when its
    source goes. A recording is started with its app, its start (the time its file
    is named after) and the meeting's metadata, and is told when the metadata changes,
    with the app and the start unchanged.
  enforcement: test
- id: INV-011
  statement: A window chosen by hand is recorded with its app's sound alone, never
    all output audio, and is never exchanged for another window. Only a recording
    of the desktop or of a window started by hand leaves the microphone out; it keeps
    its track.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: Engine::tick, which advances detection and recording by one step and
      returns the state and notices to show.
  - id: PROV-002
    statement: Commands to start a recording of a chosen detected meeting, of the
      desktop or of a chosen window (the last two with or without the microphone),
      to stop the recording, and to pause automatic recording; the windows the user
      can choose from, read when asked; a status listing the detected meetings and
      the recording.
  consumes:
  - id: CONS-001
    statement: 'SignalSource, each read on its own: the audio sessions of capture
      devices and of playback devices, the process tree, the windows, and the extension''s
      links and reported meetings.'
  - id: CONS-002
    statement: 'Recorder: start, stop, retarget, describe and take_failure.'
  - id: CONS-003
    statement: Clock, OutputFolder and Pause.
  - id: CONS-004
    statement: 'MeetingApp adapters, optional: for the app run as a process name,
      what each of its windows is (main, a meeting''s, or unknown) and what a window
      that is not the main one tells of the meeting, from their titles.'
  forbidden:
  - id: FORB-001
    statement: Reading the clock, the file system or an operating system API directly.
variability:
  fixed:
  - id: FIX-001
    statement: 'The meaning of the configuration keys start_debounce_seconds and end_grace_seconds
      (desktop apps: how long a meeting''s condition must hold to start it, and how
      long it must be absent to end it), browser_start_debounce_seconds and browser_end_grace_seconds
      (meetings the extension reports), capture_scope and of the rule fields.'
  free:
  - id: FREE-001
    statement: The default rules, the meeting app adapters and what they read, which
      of the windows that can be a meeting's is chosen, the lookback, and the retry
      timing.
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: When a recording fails, keep the partial file, raise a notice and start
    a new recording while the meeting continues.
- id: FAIL-002
  statement: When signals cannot be read, report the error for that tick and keep
    the current recording.
trust_boundaries: []
compatibility_policies:
- id: COMPAT-001
  statement: 'The configuration file names its format version. A file of another version,
    or of none, is not read: it is kept beside a new file of defaults, and the app
    starts with those. Within a version a key is not repurposed, and an unknown key
    is rejected at startup rather than ignored.'
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
- {type: references, target: adr-20261120-record-the-desktop-or-a-window-by-hand}
source_paths:
- crates/core/src/apps.rs
- crates/app/src/apps
- crates/core/src/rules.rs
- crates/core/src/metadata.rs
- crates/core/src/detector.rs
- crates/core/src/coordinator.rs
- crates/core/src/engine.rs
- crates/core/src/naming.rs
- crates/core/src/config.rs
- crates/core/src/signals.rs
- crates/core/src/recording.rs
- crates/core/src/target.rs
summary: Contract of the rules, the start and end decision, and the coordinator that
  owns the single recording.
---

## Purpose

Decide, from what can be observed about the machine, when the user is in a meeting and what exactly should be recorded for it, independent of how signals are obtained and how recording is performed.

## Responsibilities

`RuleEvaluator` maps a `SignalSnapshot` to the satisfied rule, with the process that uses the microphone resolved to its top-level owner in the process tree, and the app's windows, each with what the app's `MeetingApp` adapter tells of it. `MeetingDetector` follows each meeting (`MeetingKey`: the rule, plus the tab for an extension-reported meeting) on its own and turns the evaluations into a start, after its condition has held for the debounce, and an end, after it has been absent for the grace period. It is told on each tick whether the windows were read. While they are read without a break it notes when each window of a rule's app appeared; the windows found when reading begins count as open before any meeting. A desktop meeting's window is the largest of the app's windows that its adapter tells is a meeting's or, of those it tells nothing about, that appeared from `MEETING_WINDOW_LOOKBACK` (10 seconds) before its microphone use began, chosen at its start or, when there is none yet, as soon as there is; when it closes, a window that appears later can take its place. Only that window is recorded (INV-009); it has no part in when the meeting ends ([ADR](../adr/adr-20261119-end-a-desktop-meeting-on-the-microphone.md)). What a meeting's window or its evidence tells of it is its `MeetingMetadata`; when that changes the detector reports `Updated`, and it keeps what was learned when the window goes (INV-010). Ends and updates are reported before starts in the same tick. `RecordingCoordinator` keeps the detected meetings in start order and owns the single recording: when automatic recording is on it records a meeting that starts while nothing is recorded, and passes the recording to a waiting meeting when the recorded one ends; it builds the `RecordingRequest` with the recording's metadata (the app, the time its file is named after, the meeting's metadata), tells the recorder when the meeting's metadata changes, starts and stops the recorder, moves the video to the meeting's window when the recorded one is missing, and restarts after a failure. A manual start names a `RecordingTarget`: a detected meeting, recorded as its automatic recording would be, the desktop, or a window from the list `Engine::window_choices` reads when the panel asks (the windows the system shows as windows of their own, apart from the app's own). A window is requested with its process as the audio source and is not followed like a meeting's; the desktop and a window may leave the microphone out, which the request carries to the recorder (INV-011, [ADR](../adr/adr-20261120-record-the-desktop-or-a-window-by-hand.md)). `Engine` runs these in order once per tick. It reads the microphone's sessions and the extension's state on every tick, the playback sessions only while some process uses the microphone, and the process tree and the windows while some process uses the microphone or an app's meeting is recorded. Nothing else uses them, so an idle machine is not searched. That is also why the timing of the windows open when the microphone is taken tells nothing: Slack opens its huddle window as it takes the microphone. The detector names no app; what tells one app's windows apart differs per app and lives in its adapter in `crates/app/src/apps`, such as Slack's, which reads the mark Slack puts in its main window's title while a joined huddle is open ([ADR](../adr/adr-20261119-tell-meeting-windows-apart-through-app-adapters.md)).

## Boundaries

All of this is in `crates/core` and reaches the world only through the ports listed in the front matter. Time arrives as a value from `Clock`. The coordinator does not know whether a `VideoTarget::BrowserTab` is recorded by a browser or a window by the operating system; that is the recorder's concern.

## Invariants

The scope rule (INV-002) is the one most easily broken by a well-meant fallback. When the meeting has no window of its own yet the request carries `VideoTarget::Blank`, never another window of the app; when the meeting has no known audio process and the scope is app, no device-wide audio is requested on its behalf. Only `capture_scope = "screen"` and a manual recording of the desktop request the display and the output device; a window chosen by hand requests its app's process, never the output device. A start for a meeting that is no longer detected fails instead of turning into a recording of the screen.

Stopping a recording by hand marks its meeting (INV-008) and starts nothing else. Only two events start an automatic recording while automatic recording is on: a meeting that starts while nothing is recorded, and the end of the meeting being recorded automatically, which passes the recording to the earliest meeting that waited for it. Switching automatic recording on again clears the marks and records the earliest detected meeting.

Extension-reported meetings take precedence for the browser they come from (INV-004), so one call is not recorded both as a tab and as a window.

## Collaboration

`worker` calls `Engine::tick` at the poll interval, forwards commands from the tray and the status panel, and publishes the status they show. `LinkedSignalSource` and `LinkedRecorder` in the app present the operating system and the extension as one signal source and one recorder.

## Failure Responsibility

A failed start is retried on later ticks while the meeting lasts, with a notice the first time. A failure during recording is collected through `take_failure`, after which the coordinator starts a new file. Stopping always reaches `Recorder::stop`, including when the engine is dropped.

## Variability

Rules are user data. Which of the windows that appeared with a meeting is chosen, the lookback and the retry timing may change without changing this contract. The meaning of configuration keys is fixed.

The window that appeared with a meeting was chosen over window titles after a Slack huddle was observed on 2026-11-19: its window is titled like any other Slack window ("channel - workspace - Slack"), it opened as the huddle started and closed as it was left, Slack kept the microphone while muted, and its audio output went on for about 10 seconds after leaving, so output was not taken as a sign of a meeting.

## Conformance

`crates/core/tests/{rules,detector,coordinator,engine}.rs` run the whole component against `crates/core/src/fakes.rs`. Whether the default rules match real meeting apps is not something these tests can establish; see the open issue on verification against real services.

## Related Decisions

[ADR: record only the meeting app by default](../adr/adr-20261117-record-only-the-meeting-app.md). User-facing behavior is stated in `PRODUCT.md`, and the keys in `docs/usage/configuration.md`.
