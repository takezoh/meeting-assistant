---
id: design-browser-tab-recording
kind: design
title: Browser tab recording
status: active
created: '2026-11-17'
scope_type: component
responsibilities:
- id: RESP-001
  statement: Detect in the browser that a page on a supported meeting site holds a
    connected call and report it to the app.
- id: RESP-002
  statement: Record one tab's video and audio inside the browser on the app's command
    and deliver the data to the app.
- id: RESP-003
  statement: Track extension connections, reported meetings and the single tab recording,
    and store its data in a file.
- id: RESP-004
  statement: Present the extension path and the operating system path as one signal
    source and one recorder.
- id: RESP-005
  statement: Record the user's microphone in the app beside a tab recording, and add
    it to the tab's file as its second audio track when the tab recording ends.
invariants:
- id: INV-001
  statement: The app holds at most one tab recording across all connections.
  enforcement: test
- id: INV-002
  statement: A tab recording is reported as started only after the extension's upload
    has connected and proved itself.
  enforcement: test
- id: INV-003
  statement: When a connection is lost during a recording, the recording ends as failed
    and the received data stays in the file.
  enforcement: test
- id: INV-004
  statement: Recording data travels only on the upload WebSocket, as binary messages
    appended to the file as they are; the control channel carries no recording data.
  enforcement: test
- id: INV-005
  statement: Both listeners accept loopback connections only, and read a new connection
    only after the operating system has named its other end as a process of the current
    user; other connections are closed unread. Then the control listener requires
    the relay token in the first message; each upload listener requires the extension's
    Origin and that recording's one-time token, and closes once one connection has
    proved itself.
  enforcement: test
- id: INV-006
  statement: The extension ID, the host name and the message types are identical in
    the app and in the extension.
  enforcement: test
- id: INV-007
  statement: The microphone recording starts before the extension is asked to start,
    so that it covers the whole tab recording, and is stopped and added after the tab
    recording has ended or failed. A tab that cannot start leaves no microphone file.
  enforcement: test
- id: INV-008
  statement: Adding the microphone copies every byte of the tab recording's fragments
    and places the microphone on its timeline, whose zero is when the upload proved
    itself. When adding fails, the tab's file is as the browser wrote it and the microphone
    file stays beside it.
  enforcement: test
- id: INV-009
  statement: A tab is captured at its size in device pixels, scaled down to fit in the
    video.width by video.height that the app sends with start, with its aspect ratio
    kept and never scaled up, and at that size when the tab's size cannot be read.
  enforcement: test
boundaries:
  provides:
  - id: PROV-001
    statement: 'BrowserPort: snapshot of links and meetings, start_capture (the file
      and the recording''s time zero), stop_capture, take_failure.'
  - id: PROV-002
    statement: The wire protocol in docs/technical/browser-link.md.
  consumes:
  - id: CONS-001
    statement: 'Browser APIs: native messaging, tabCapture, offscreen documents, MediaRecorder,
      RTCPeerConnection.'
  - id: CONS-002
    statement: The browser switch --allowlisted-extension-id for capture without a
      user gesture.
  - id: CONS-003
    statement: 'LoopbackPeers: the user that owns the other end of a loopback connection,
      and the user the app runs as.'
  - id: CONS-004
    statement: 'MicrophoneRecorder: the capture pipeline on its microphone-only plan.
      MicrophoneMerge: recording::merge, with recording::mp4 behind its Remuxer and
      the file system behind RecordingFiles.'
  forbidden:
  - id: FORB-001
    statement: Socket or file access from browser/hub or browser/protocol.
  - id: FORB-002
    statement: Recording decisions made in the extension; it records only on the app's
      command.
variability:
  fixed:
  - id: FIX-001
    statement: The extension ID lnplioaidiniokndppmckdaflgefgale, fixed by the manifest
      key, and the host name com.meeting_assistant.host.
  - id: FIX-002
    statement: Length-prefixed JSON framing for control messages and binary WebSocket
      messages for the recording, as described in docs/technical/browser-link.md.
      Neither side keeps compatibility with older versions of the other.
  free:
  - id: FREE-001
    statement: How a call is recognized in a page, the supported sites, encoder settings
      and chunk interval.
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: The extension reports a start or recording failure as an error message
    with its cause; the app surfaces it as a recorder failure and logs it.
- id: FAIL-002
  statement: When the extension link cannot be started, the app runs without it and
    the operating system path keeps working.
- id: FAIL-004
  statement: A tab recording that fails to start leaves no capture of the tab that this
    extension could release, and a start refused because one is already recording leaves
    that recording as it is. The stream ID is asked for only once the recording page exists
    and is used at once; any track captured is stopped on failure and the page closed. A
    capture the browser still holds from elsewhere is reported as such, with what releases it.
- id: FAIL-003
  statement: A microphone that cannot be started leaves the tab recorded with its own
    audio only; a tab recorded as WebM, or a failed merge, leaves the microphone file
    beside the recording. Each is logged, and none fails the recording.
trust_boundaries:
- id: TRUST-001
  statement: The browser starts the relay only for the extension ID in the native-host
    manifest; the app additionally requires the token from a file only the current
    user's processes are expected to read.
- id: TRUST-002
  statement: Page scripts are untrusted; the page hook reports only call state, and
    no page content is executed or interpreted by the app.
- id: TRUST-003
  statement: 'Any process on the computer can connect to a loopback port. The trust
    boundary is the current user: another user''s connections are closed before anything
    is read from them, so they cannot take the places kept for connections that have
    not presented a token. The current user''s processes are trusted not to flood
    the listeners; they can already read the token file.'
compatibility_policies:
- id: COMPAT-001
  statement: 'None between versions: the extension is written out by the same executable
    that runs the app, and neither keeps compatibility with an older version of the
    other (FIX-002).'
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-tab-recording-through-extension}
- {type: references, target: adr-20261118-admit-loopback-connections-only-from-the}
source_paths:
- crates/app/src/browser
- crates/app/src/linked.rs
- crates/app/src/bundle.rs
- extension
- docs/technical/browser-link.md
- crates/app/src/platform/mac/peers.rs
- crates/app/src/platform/win/peers.rs
- crates/app/src/recording/merge.rs
- crates/app/src/recording/mp4.rs
summary: Contract between the extension, the relay and the app for detecting browser
  calls and recording a single tab.
---

## Purpose

Record a browser meeting as its tab rather than its window, and detect it from the call itself rather than from microphone use, without asking the user to click anything when a meeting starts.

## Responsibilities

In the browser, `hook.js` observes `RTCPeerConnection` in the page, `bridge.js` forwards the state, `background.js` reports meetings and executes start and stop, and `offscreen.js` records the tab's video and audio with `MediaRecorder`. It does not record the microphone: Chrome 157 refuses it to the offscreen document even with the permission granted ([the issue](../issue/issue-20261118-the-extension-cannot-capture-the-microph.md)), so the app records it. The relay is the same executable started by the browser as a native-messaging host; it only copies frames. In the app, `protocol` defines messages and framing, `hub` is the state machine, `server` owns the socket, the threads and the file, and `linked` routes a `VideoTarget::BrowserTab` request to the extension and everything else to the platform recorder.

## Boundaries

The hub decides and the server acts: the hub receives parsed control messages and the upload's events, and returns the commands to send; it writes recording bytes only to a `ChunkSink`. The recording goes from the offscreen document to the app over its own WebSocket rather than through native messaging, which carries only JSON, so the data is never base64-encoded or copied through the service worker and the relay. See [ADR: binary upload](../adr/adr-20261118-send-tab-recordings-to-the-app-over-a-we.md). The extension holds no policy. It reports what it sees and records when told, so that the rules for when to record stay in `crates/core`.

A tab recording does not pass through the capture pipeline. Its video and audio settings are the extension's, not `config.toml`'s, except the size of its picture (INV-009). `start` carries `video.width` and `video.height` as `max_size`. The extension computes the tab's size in device pixels, scaled down to fit in `max_size`, and gives that size to the browser as the capture's maximum. The browser does the one scaling while it captures, and the app copies what it records.

The bound is not passed as it is. In Chrome and Edge 158 the browser delivered every picture at exactly the size it was given, and fitted the tab into it with black bars and scaled up when it was smaller ([the note](../note/note-20261125-tab-capture-sizes-in-chrome-and-edge.md)). Passing `max_size` itself would therefore put bars beside a tab of another shape and scale a smaller tab up. With no size the pictures came at the screen's. The picture keeps its first size for the whole recording. When the tab changes size, the browser fits it into the picture again.

The user's microphone does: `LinkedRecorder` starts the pipeline's microphone-only recording beside the tab's file (`<recording>.microphone.m4a`) before it asks the extension to start, keeps the moment the upload proved itself as the tab's time zero, and after the tab recording ends hands both to `MicrophoneMerge`. `recording::merge` decides where the microphone goes (the part before the tab's zero is dropped, the rest cut at the tab's end) and that the result replaces the tab's file only once it is complete. `recording::mp4` writes it by rewriting the tab's `moov` and appending fragments for the new track, copying the tab's own fragments byte for byte. Re-muxing through AVFoundation was tried and rejected: it reads the browser's fragment timing differently from the fragments' `tfdt` and shortened the video of a 13-minute recording by 9.6 seconds.

After the merge the tab's file gets what is known of the meeting written into it, by the procedure every recording goes through ([ADR](../adr/adr-20261119-write-meeting-metadata-into-the-recording.md)); for a tab that is what the extension sent with the meeting (`meeting_code` and `page_title` for Google Meet, `page_title` otherwise), with the service and the recording's start.

## Invariants

The fixed identifiers are checked by tests in `bundle.rs` (the host name in `background.js`, the presence of the manifest key) and in `install` (the native-host manifest names this extension only). A mismatch would make the browser refuse to start the relay.

## Collaboration

`LinkedSignalSource` adds the hub's links and meetings to each snapshot. Core excludes a browser that has a link from window-based detection and turns a reported meeting into a recording request for that tab. `--install` writes the extension's files, registers the native host per browser and creates the launcher that adds the browser switch.

## Failure Responsibility

Without the switch, `tabCapture` refuses and the extension sends an error that names the switch. A lost connection fails the recording; the coordinator may then start again if the meeting is still reported. If the listener cannot start, the app logs it and continues without tab recording.

Asking for a stream ID registers a capture of the tab in the browser, and Chromium refuses another capture of that tab while the registered one is pending or running ("Cannot capture a tab with an active stream"). Reading Chromium's `tab_capture_registry.cc` on 2026-11-19, such a capture leaves that state only through the media request reporting an end, stop or error, the tab being closed, or the extension being unloaded; there is no timeout, and nothing an extension calls releases it. So the extension orders a start to leave nothing running when it fails (FAIL-004): the recording page is made and the tab's size read before the stream ID is asked for, the ID is used at once, and the page stops every captured track and is closed on failure. A capture left pending by the browser itself, as `--use-fake-ui-for-media-stream` did on 2026-11-18, cannot be released this way; the extension then says so and that closing the tab releases it. Reloading the extension would also release it, but would cut the meeting tab off from the extension until the page is loaded again.

## Variability

Detection inside the page is a heuristic and may be replaced per site. The container is whatever the browser can produce, MP4 preferred and WebM otherwise; the app names the file after the container as soon as the extension reports it, before it reports the recording as started, so that the notices and the status panel name the file that is written.

## Conformance

`browser/{protocol,hub}.rs`, `linked.rs`, `recording/{merge,mp4}.rs` and `bundle.rs` carry the unit tests; `linked.rs` checks the order of starting, stopping and merging and what each failure leaves. INV-009 is checked from both sides: `browser/server.rs` checks that a start names the largest picture, and `extension/test/background.test.mjs` checks the size the extension then captures at. What the browser does with that size was observed in a browser, not tested. The real path through a browser was checked by hand in Chrome and Edge and is recorded in the verification note; it is not covered by automated tests.

## Related Decisions

[ADR: record browser meetings per tab through an extension](../adr/adr-20261117-tab-recording-through-extension.md).
