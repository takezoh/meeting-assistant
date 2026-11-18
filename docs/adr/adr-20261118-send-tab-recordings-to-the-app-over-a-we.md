---
id: adr-20261118-send-tab-recordings-to-the-app-over-a-we
kind: adr
title: Send tab recordings to the app over a WebSocket as binary
status: accepted
created: '2026-11-18'
decision_makers:
- user
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-tab-recording-through-extension}
source_paths:
- extension/offscreen.js
- extension/background.js
- crates/app/src/browser/server.rs
- crates/app/src/browser/hub.rs
- crates/app/src/browser/protocol.rs
summary: The recording goes from the offscreen document straight to a per-recording
  loopback WebSocket as binary messages; native messaging carries only control messages.
updated: '2026-11-18'
---

# Send tab recordings to the app over a WebSocket as binary

## Context

Chrome native messaging carries only JSON. The recording therefore went from `MediaRecorder` to an `ArrayBuffer`, was base64-encoded in JavaScript, passed from the offscreen document to the service worker and on to the browser as JSON, copied by the relay process, parsed by the app and decoded back to bytes. On 2026-11-18, in Chrome 157 on a Mac, the hand-written base64 encoder took 21.7 ms per MiB against 0.18 ms for the built-in one. The data is about 0.33 MB per second, so the cost was small, but every byte was copied and converted several times for no benefit. The user asked whether the browser could hand over binary and said compatibility with older Chrome and older versions is not needed.

## Decision

The offscreen document sends the recording to the app over a WebSocket, as binary messages written to the file as they arrive. For each recording the app opens a loopback listener on a fresh port with a one-time token and sends both in `start` over native messaging. The app accepts the WebSocket only with the extension's `Origin` and that token, then closes the listener. A normal close ends the recording; a close with code 1011 fails it with the reason. Native messaging keeps the control messages only: meetings, start, stop and start failures. The old `started`, `chunk`, `stopped` and `warning` messages are removed rather than kept alongside.

## Consequences

- Positive: no base64, no JSON around the data, no hop through the service worker and the relay; the bytes `MediaRecorder` produces reach the file through one socket.
- Positive: a recording counts as started only when its data channel is connected, which is a stronger guarantee than a message saying so.
- Negative: a second listener exists while a recording starts. It is loopback only, short-lived, and requires both the extension's origin and a one-time token.
- Negative: an older extension cannot work with this app, and the reverse; both are installed together.
- Neutral: whether Chrome's Local Network Access restrictions apply to an extension page connecting to `ws://127.0.0.1` was not established on 2026-11-18. The WebSocket ordering the design relies on (messages sent before `close()` arrive before the close) was observed in Chrome 157.

## Confirmation

`crates/app/src/browser/server.rs` tests drive the upload listener with a real WebSocket client: data written in order, a wrong origin or token refused without ending the wait, a failure close, and a close from the app's side. `crates/app/src/browser/protocol.rs` tests that the removed messages no longer parse.
