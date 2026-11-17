# Browser link reference

This page describes the wire contract between the browser extension and the app: the processes involved, how a connection is established, the framing and the messages. The [browser tab recording design](../design/design-browser-tab-recording.md) owns the invariants; `crates/app/src/browser/protocol.rs` and `extension/background.js` are the two ends and must change together.

## Parties

| Party | Role |
| --- | --- |
| `hook.js` | Runs in the page's own JavaScript world. Wraps `RTCPeerConnection` and reports whether a connected call exists |
| `bridge.js` | Content script. Forwards the call state from the page to the extension |
| `background.js` | Service worker. Tracks meetings per tab, holds the native-messaging port, obtains the tab-capture stream ID and drives the offscreen document |
| `offscreen.js` | Offscreen document. Records the tab and the microphone with `MediaRecorder` and emits chunks |
| Relay | The `meeting-assistant` executable started by the browser as native-messaging host `com.meeting_assistant.host`. Copies frames between its standard input/output and the app's socket |
| App | Listens on a loopback TCP port, keeps the connection state in `Hub`, writes recording data to the file |

## Connection

1. At startup the app binds `127.0.0.1` on a port chosen by the OS, generates a random token and writes `{ "port", "token" }` to `browser-link.json` in the user's local application data.
2. The extension calls `chrome.runtime.connectNative("com.meeting_assistant.host")`. The browser looks up the host manifest that `--install` registered, checks that the calling extension's ID is in its `allowed_origins`, and starts the executable with the extension origin `chrome-extension://lnplioaidiniokndppmckdaflgefgale/` as the first argument.
3. The executable recognizes that argument and runs as the relay. It reads `browser-link.json`, connects, and sends `hello` with the token and its process ID.
4. The app closes a connection whose first message is not a `hello` with the right token. Any local process can reach the loopback port, so until the token has been presented a connection is limited: the first message may be at most 4 KiB, it must arrive within 5 seconds, and at most 8 connections may be waiting at once. The token is compared in constant time.

The process ID in `hello` lets the app find which browser process tree the connection belongs to, so that this browser is excluded from window-based detection.

If the app is not running, the relay cannot connect and exits; the extension retries the connection later.

## Framing

Each message is a 32-bit length in native byte order followed by that many bytes of UTF-8 JSON, the same framing as Chrome native messaging. A frame longer than 64 MiB is rejected. Every message is an object with a `type` field.

## Messages from the extension

| `type` | Fields | Meaning |
| --- | --- | --- |
| `hello` | `token`, `host_pid` | Sent by the relay first |
| `meeting` | `tab`, `active`, `service` (optional) | A call became connected or ended in a tab. `service` names the site and is used in the file name |
| `started` | `tab`, `mime` | Recording of the tab has started with this container and codecs |
| `chunk` | `tab`, `data` | The next piece of the recording, base64 encoded |
| `stopped` | `tab` | The recorder has stopped and every chunk has been sent |
| `error` | `tab` (optional), `message` | Starting or continuing the recording failed |
| `warning` | `message` | Something degraded, for example the microphone could not be opened; recording continues |

An unknown `type` is ignored so that a newer extension works with an older app.

## Messages to the extension

| `type` | Fields | Meaning |
| --- | --- | --- |
| `start` | `tab` | Start recording this tab |
| `stop` | `tab` | Stop recording and send the remaining data |

## Recording sequence

```
app                         extension
 | <- meeting(tab, active) -- |   call connected
 | -- start(tab) -----------> |
 | <- started(tab, mime) ---- |   start_capture returns to the coordinator
 | <- chunk(tab, data) ------ |   about once per second, appended to the file
 | -- stop(tab) ------------> |
 | <- chunk ... ------------- |
 | <- stopped(tab) ---------- |   file is closed
```

- The app holds at most one tab recording. A `start` is sent only when none is in progress.
- `start_capture` fails if `started` or `error` does not arrive within the start timeout, and the coordinator treats that like any failed start.
- A `chunk` for a tab that is not being recorded is dropped.
- If the connection closes while recording, the recording ends as failed and the bytes received so far remain in the file.
- The extension prefers MP4 (H.264 and AAC) and falls back to WebM when the browser cannot produce MP4. The app names the file by the reported container.

## Capture without a user gesture

`chrome.tabCapture.getMediaStreamId` normally requires that the user has invoked the extension on that tab. The browser switch `--allowlisted-extension-id=<id>` removes that requirement for the named extension. Without it the call fails, and the extension reports an `error` whose message tells the user to start the browser with the switch. See [the decision](../adr/adr-20261117-tab-recording-through-extension.md).
