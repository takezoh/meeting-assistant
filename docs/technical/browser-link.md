# Browser link reference

This page describes the wire contract between the browser extension and the app: the processes involved, how the connections are established, the framing and the messages. The [browser tab recording design](../design/design-browser-tab-recording.md) owns the invariants; `crates/app/src/browser/protocol.rs` on one side and `extension/background.js` and `extension/offscreen.js` on the other are the two ends and change together. Neither side keeps compatibility with older versions of the other.

There are two channels. Control messages (meetings, start, stop, start failures) travel through Chrome native messaging and the relay process. The recording itself travels from the offscreen document straight to the app over a WebSocket, as binary messages, so that it is neither base64-encoded nor wrapped in JSON.

## Parties

| Party | Role |
| --- | --- |
| `hook.js` | Runs in the page's own JavaScript world. Wraps `RTCPeerConnection` and reports whether a connected call exists; a call that was connected counts while it is reconnecting, until it has failed or closed |
| `bridge.js` | Content script. Forwards the call state and the page title from the page to the extension |
| `background.js` | Service worker. Tracks meetings per tab, holds the native-messaging port, obtains the tab-capture stream ID and drives the offscreen document |
| `offscreen.js` | Offscreen document. Records the tab's video and audio with `MediaRecorder` and sends the recording to the app over the upload WebSocket. It does not use the microphone; the app records it |
| Relay | The `meeting-assistant` executable started by the browser as native-messaging host `com.meeting_assistant.host`. Copies control frames between its standard input/output and the app's socket |
| App | Listens on a loopback TCP port for the relay, opens a second loopback listener for each recording's upload, keeps the state in `Hub` and writes the recording to the file |

## Control connection

1. At startup the app binds `127.0.0.1` on a port chosen by the OS, generates a random token and writes `{ "port", "token" }` to `browser-link.json` in the user's local application data.
2. The extension calls `chrome.runtime.connectNative("com.meeting_assistant.host")`. The browser looks up the host manifest that `--install` registered, checks that the calling extension's ID is in its `allowed_origins`, and starts the executable with the extension origin `chrome-extension://lnplioaidiniokndppmckdaflgefgale/` as the first argument.
3. The executable recognizes that argument and runs as the relay. It reads `browser-link.json`, connects, and sends `hello` with the token and its process ID.
4. The app first asks the operating system which user owns the other end of the new connection: on macOS from the kernel's list of TCP connections (`net.inet.tcp.pcblist64`), on Windows from the TCP table (`GetExtendedTcpTable`) and that process's token. Unless it is the user the app runs as, the connection is closed before anything is read. The app then closes a connection whose first message is not a `hello` with the right token. Any of the user's processes can reach the loopback port, so until the token has been presented a connection is limited: the first message may be at most 4 KiB, it must arrive within 5 seconds, and at most 8 connections may be waiting at once. The token is compared in constant time.

The process ID in `hello` lets the app find which browser process tree the connection belongs to, so that this browser is excluded from window-based detection.

If the app is not running, the relay cannot connect and exits; the extension retries the connection later.

Each control message is a 32-bit length in native byte order followed by that many bytes of UTF-8 JSON, the same framing as Chrome native messaging. A frame longer than 64 MiB is rejected. Every message is an object with a `type` field; a message the app cannot parse is logged and ignored.

### Messages from the extension

| `type` | Fields | Meaning |
| --- | --- | --- |
| `hello` | `token`, `host_pid` | Sent by the relay first |
| `meeting` | `tab`, `active`, `service`, `metadata` | A call became connected or ended in a tab. `service` and `metadata` come with a start. `service` names the site and is used in the file name. `metadata` is what the extension tells of the meeting, as `[key, value]` pairs in order, and differs per service: `meeting_code` and `page_title` for Google Meet, `page_title` otherwise. The first value tells two meetings of one service apart and is shown. The app keeps keys of lowercase letters, digits and `_`, trims values and cuts them at 200 characters, keeps at most 8 pairs, and writes them into the recording |
| `error` | `tab` (optional), `message` | Recording the tab could not start |

### Messages to the extension

| `type` | Fields | Meaning |
| --- | --- | --- |
| `start` | `tab`, `upload`, `token`, `max_size` | Record this tab and send the recording to the WebSocket at `upload` (`ws://127.0.0.1:<port>/`), presenting `token` first. `max_size` is `{ "width", "height" }`, the app's `video.width` and `video.height`: the extension captures the tab at its size in device pixels, scaled down to fit in it with its aspect ratio kept and never up, and at `max_size` when the tab's size cannot be read. The tab's size in device pixels is its size from `chrome.tabs.get` times its page's `devicePixelRatio` divided by the tab's zoom, which that ratio includes. A start without a whole positive `max_size` is answered with an `error` saying that the app and the extension are of different versions |
| `stop` | `tab` | Stop recording, send the remaining data and close the upload |

## Upload connection

For each recording the app binds a new loopback listener on a port chosen by the OS and generates a one-time token, both sent in `start`. The listener accepts connections only while the recording is starting and closes once one has proved itself. As on the control listener, a connection not made by one of the current user's processes is closed before anything is read.

1. The offscreen document opens a WebSocket to `upload`. The app completes the handshake only if the `Origin` header is the extension's, `chrome-extension://lnplioaidiniokndppmckdaflgefgale`.
2. The first message is text: `{ "token", "mime" }`, sent right after `MediaRecorder` starts. The app takes the moment it accepts this message as the recording's time zero, and places the microphone it records itself on that timeline. A wrong token or anything else closes the connection, and the app keeps waiting for the right one. Until then a message may be at most 4 KiB, and the handshake and this message must complete within 5 seconds of connecting. Connections that have not presented the token are served side by side, at most 8 at once; a ninth makes the one that has waited longest close. A local process that connects and sends slowly or nothing therefore cannot keep the offscreen document out.
3. The app then treats the recording as started: `start_capture` returns to the coordinator, and the file is renamed to the extension of the container in `mime`, choosing a free name instead of replacing an existing file.
4. Binary messages follow, about once per second, each the next piece of the recording; the app appends them to the file as they are. The app ignores any later text message.
5. The offscreen document closes the WebSocket after the last piece: with code 1000 when the recording ended normally, or 1011 with the reason when it failed. Messages sent before the close are delivered before it. The document tells `background.js` that it has finished only once the app has acknowledged the close, so that closing the document cannot cut off data still being sent.

If no message arrives for 30 seconds, or the connection breaks without a close, the recording ends as failed and the bytes received so far remain in the file. If the app ends the recording on its side, it closes the upload, and the offscreen document stops recording.

## Recording sequence

```
app                              extension
 | <- meeting(tab, active) ------- |   call connected (control)
 |   microphone recording starts   |   <recording>.microphone.m4a, on the app's side
 | -- start(tab, ..., max_size) -> |   (control)
 | <== WebSocket to upload ======= |   Origin checked
 | <== text {token, mime} ======== |   MediaRecorder started: time zero; start_capture returns to the coordinator
 | <== binary ==================== |   about once per second, appended to the file
 | -- stop(tab) -----------------> |   (control)
 | <== binary ... ================ |
 | <== close 1000 ================ |   file is closed
 |   microphone recording stops    |   added to the file as its second audio track
```

- The app holds at most one tab recording. A `start` is sent only when none is in progress.
- `start_capture` fails if the upload does not prove itself, or `error` arrives, within the start timeout, and the coordinator treats that like any failed start.
- If the control connection closes while recording, the recording ends as failed and the bytes received so far remain in the file.
- The extension prefers MP4 (H.264 and AAC) and records WebM when the browser cannot produce MP4.

## Capture without a user gesture

`chrome.tabCapture.getMediaStreamId` normally requires that the user has invoked the extension on that tab. The browser switch `--allowlisted-extension-id=<id>` removes that requirement for the named extension. Without it the call fails, and the extension reports an `error` whose message tells the user to start the browser with the switch. See [the decision](../adr/adr-20261117-tab-recording-through-extension.md).
