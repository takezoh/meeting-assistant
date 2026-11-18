---
id: design-browser-tab-recording
kind: design
title: Browser tab recording
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: "Detect in the browser that a page on a supported meeting site holds a connected call and report it to the app."
- id: RESP-002
  statement: "Record one tab and the microphone inside the browser on the app's command and deliver the data to the app."
- id: RESP-003
  statement: "Track extension connections, reported meetings and the single tab recording, and store its data in a file."
- id: RESP-004
  statement: "Present the extension path and the operating system path as one signal source and one recorder."
invariants:
- id: INV-001
  statement: "The app holds at most one tab recording across all connections."
  enforcement: test
- id: INV-002
  statement: "A tab recording is reported as started only after the extension has confirmed it."
  enforcement: test
- id: INV-003
  statement: "When a connection is lost during a recording, the recording ends as failed and the received data stays in the file."
  enforcement: test
- id: INV-004
  statement: "A message of an unknown type is ignored."
  enforcement: test
- id: INV-005
  statement: "The listener accepts loopback connections only and requires the token in the first message."
  enforcement: review
- id: INV-006
  statement: "The extension ID, the host name and the message types are identical in the app and in the extension."
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: "BrowserPort: snapshot of links and meetings, start_capture, stop_capture, take_failure."
  - id: PROV-002
    statement: "The wire protocol in docs/technical/browser-link.md."
  consumes:
  - id: CONS-001
    statement: "Browser APIs: native messaging, tabCapture, offscreen documents, MediaRecorder, RTCPeerConnection."
  - id: CONS-002
    statement: "The browser switch --allowlisted-extension-id for capture without a user gesture."
  forbidden:
  - id: FORB-001
    statement: "Socket or file access from browser/hub or browser/protocol."
  - id: FORB-002
    statement: "Recording decisions made in the extension; it records only on the app's command."
variability:
  fixed:
  - id: FIX-001
    statement: "The extension ID lnplioaidiniokndppmckdaflgefgale, fixed by the manifest key, and the host name com.meeting_assistant.host."
  - id: FIX-002
    statement: "Length-prefixed JSON framing and the existing message types and fields."
  free:
  - id: FREE-001
    statement: "How a call is recognized in a page, the supported sites, encoder settings and chunk interval."
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: "The extension reports a start or recording failure as an error message with its cause; the app surfaces it as a recorder failure and logs it."
- id: FAIL-002
  statement: "When the extension link cannot be started, the app runs without it and the operating system path keeps working."
trust_boundaries:
- id: TRUST-001
  statement: "The browser starts the relay only for the extension ID in the native-host manifest; the app additionally requires the token from a file only the current user's processes are expected to read."
- id: TRUST-002
  statement: "Page scripts are untrusted; the page hook reports only call state, and no page content is executed or interpreted by the app."
compatibility_policies:
- id: COMPAT-001
  statement: "New message types and optional fields may be added; existing ones keep their meaning so that an extension and an app of different versions degrade instead of failing."
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-tab-recording-through-extension}
source_paths:
- crates/app/src/browser
- crates/app/src/linked.rs
- crates/app/src/bundle.rs
- extension
- docs/technical/browser-link.md
summary: Contract between the extension, the relay and the app for detecting browser calls and recording a single tab.
---

## Purpose

Record a browser meeting as its tab rather than its window, and detect it from the call itself rather than from microphone use, without asking the user to click anything when a meeting starts.

## Responsibilities

In the browser, `hook.js` observes `RTCPeerConnection` in the page, `bridge.js` forwards the state, `background.js` reports meetings and executes start and stop, and `offscreen.js` records with `MediaRecorder`. The relay is the same executable started by the browser as a native-messaging host; it only copies frames. In the app, `protocol` defines messages and framing, `hub` is the state machine, `server` owns the socket, the threads and the file, and `linked` routes a `VideoTarget::BrowserTab` request to the extension and everything else to the platform recorder.

## Boundaries

The hub decides and the server acts: the hub receives parsed messages and returns the commands to send; it writes recording bytes only to a `ChunkSink`. The extension holds no policy. It reports what it sees and records when told, so that the rules for when to record stay in `crates/core`.

A tab recording does not pass through the capture pipeline. Its video and audio settings are the extension's, not `config.toml`'s.

## Invariants

The fixed identifiers are checked by tests in `bundle.rs` (the host name in `background.js`, the presence of the manifest key) and in `install` (the native-host manifest names this extension only). A mismatch would make the browser refuse to start the relay.

## Collaboration

`LinkedSignalSource` adds the hub's links and meetings to each snapshot. Core excludes a browser that has a link from window-based detection and turns a reported meeting into a recording request for that tab. `--install` writes the extension's files, registers the native host per browser and creates the launcher that adds the browser switch.

## Failure Responsibility

Without the switch, `tabCapture` refuses and the extension sends an error that names the switch. A lost connection fails the recording; the coordinator may then start again if the meeting is still reported. If the listener cannot start, the app logs it and continues without tab recording.

## Variability

Detection inside the page is a heuristic and may be replaced per site. The container is whatever the browser can produce, MP4 preferred and WebM otherwise; the app names the file after the container as soon as the extension reports it, before it reports the recording as started, so that the notices and the status panel name the file that is written.

## Conformance

`browser/{protocol,hub}.rs`, `linked.rs` and `bundle.rs` carry the unit tests. The real path through a browser was checked by hand in Chrome and Edge and is recorded in the verification note; it is not covered by automated tests.

## Related Decisions

[ADR: record browser meetings per tab through an extension](../adr/adr-20261117-tab-recording-through-extension.md).
