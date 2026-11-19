---
id: note-20261119-two-audio-tracks-in-every-recording-desi
kind: note
title: 'Two audio tracks in every recording: design'
status: draft
created: '2026-11-19'
topic: plan
summary: Plan for two audio tracks (other side, microphone) in every recording, the
  OS microphone in tab recordings, and removing the extension's microphone capture.
tags: []
owners: []
relations:
- {type: references, target: design-capture-pipeline}
- {type: references, target: design-browser-tab-recording}
- {type: references, target: issue-20261118-the-extension-cannot-capture-the-microph}
source_paths:
- crates/app/src/recording
- crates/app/src/linked.rs
- crates/app/src/browser
- crates/app/src/platform/mac/recorder
- crates/app/src/platform/win/recorder
- extension
---

## Summary

The user decided on 2026-11-19 that every recording keeps the other side and the user's microphone as two separate audio tracks, because recordings are analysed into minutes and not played directly, and that the extension's microphone permission, which Chrome 157 refuses anyway, is removed. Browser tab recordings get the microphone from the operating system, recorded by the app beside the tab and added to the tab's file as a second track when the recording ends.

This note is the plan. Each contract below moves into [the capture pipeline design](../design/design-capture-pipeline.md) or [the browser tab recording design](../design/design-browser-tab-recording.md) in the unit that implements it.

## Notes

### File contract

- Every recording made by the app has exactly two audio tracks, in this order: **track 1, the other side** (the meeting app's audio, all output audio for a screen recording, or the tab's audio), **track 2, the microphone**.
- Both tracks start at the start of the video and share its timeline.
- A track whose source is turned off in `config.toml`, cannot be opened or fails during the recording holds silence for that time. The layout never changes, so analysis can rely on track 2 being the user. A failed source is still not replaced by a wider one.
- Audio between the backend and the pipeline stays 48 kHz, two-channel, 16-bit PCM (capture pipeline FIX-001); each track is encoded as AAC.
- Exception for tab recordings: when the microphone recording cannot be started at all, or the tab's container is not MP4, the tab file keeps only its own audio track. This is logged as a warning naming the reason; the meeting is still recorded.

### Desktop and screen recordings (capture pipeline)

- `plan` assigns sources to roles: `Track::OtherSide` takes `ProcessAudio` or `DeviceAudio`, `Track::Microphone` takes `Microphone`. `MediaFormat` lists the tracks the writer opens instead of `has_audio`.
- The pipeline keeps one PCM block per track. Each step fills each block from that track's source, or with silence, and writes it with `write_audio(writer, track, pcm, time, duration)`. Sources are no longer summed. `mixer::SourceBuffer` stays as each source's jitter buffer.
- Writers open one AAC input or stream per listed track, in order: AVAssetWriter inputs on macOS, Media Foundation sink writer streams on Windows. Neither puts the tracks in an alternate group, so both are enabled.

### Recording the microphone alone

- A `Capture` plan without video and with the single track `Microphone` records the microphone alone to an audio-only MP4 (`.m4a`). It uses the same pipeline, session and backends; the pipeline skips video when the plan has none, and the writers accept a format without video.
- `MicrophoneRecorder` (port in `linked.rs`): `start(path) -> Instant` returns once the file is being written, with the moment its audio timeline starts; `stop()` finalizes. The real adapter is a `ThreadedRecorder` running the microphone-only plan. It keeps INV-003 (finalized on stop, failure and drop) and INV-004 (a microphone that cannot be opened gives silence, nothing wider).

### Tab recordings

- Owner: `LinkedRecorder`, which already routes a `BrowserTab` request to the extension. It gains a `MicrophoneRecorder` and the merge procedure.
- Start order: start the microphone recording beside the tab file, then `start_capture`. Only when the tab capture has started is the recording reported as started (browser tab recording INV-002 unchanged). When `start_capture` fails, stop the microphone recording and delete its file.
- Alignment: `BrowserPort::start_capture` returns the file and the `Instant` at which the app accepted the upload's hello. The extension sends the hello immediately after `MediaRecorder.start()`, so this is the tab's time zero within a few milliseconds of local socket delay. The microphone's `Instant` is its time zero. A pure function turns the two into a placement: the part of the microphone before the tab's zero is dropped (or, if the microphone started later, the track is delayed), and the microphone is cut at the tab's end. Expected error: the jitter buffer's 40 ms plus the hello's delay; enough for minutes, not for lip sync.
- Where the microphone is kept: `<stem>.microphone.m4a` beside the recording, written as it is captured, so nothing is lost if the app ends abruptly.
- Stop and failure order: stop or fail the tab capture first, then stop the microphone, then merge. A tab failure still merges what was recorded, then reports the failure.
- Merge: a shared procedure (`recording/merge.rs`) writes `<stem>.merging.mp4` through the `Remuxer` port, then replaces the recording with it and removes the microphone file. `Remuxer::add_audio_track(recording, microphone, placement, output)` copies every track of the recording and the microphone's AAC samples without re-encoding. File operations go through a small port with a real adapter and a fake.
- Revised during implementation: the remuxer is shared code (`recording/mp4.rs`), not one per OS. The first macOS remuxer, an AVFoundation passthrough export, was run on a real 13-minute Chrome 157 recording on 2026-11-19 and moved its timestamps: the video's last frame went from 791.36 s to 781.76 s and the tab's audio lost 0.68 s and two packets, because AVFoundation reads the browser's fragment timing differently from the fragments' `tfdt`. The browser's file is a fragmented MP4 whose fragments locate their data relative to themselves, so the shared remuxer rewrites only the `moov` and appends the microphone as new fragments, copying the tab's fragments byte for byte. On the same recording the result kept every packet and timestamp of the tab's tracks and took under a second.
- What remains when the merge fails: the recording untouched, the microphone file beside it, the partial `.merging.mp4` removed, and a warning that names both files. The recording itself is not reported as failed.
- The merge runs inside `stop`, so quitting, updating, uninstalling and a terminate request (which all stop the recording) merge before the process exits. If the process is killed instead, the two files stay separate.

### Extension and wire contract

- `offscreen.js` records the tab's video and audio only: no `getUserMedia({ audio: true })`, no `AudioContext` mixing; the tab's sound is still played back so the user hears the meeting. It sends the upload hello after `recorder.start()`.
- The options page loses its Microphone section, `options.js` its permission code. The page keeps the explanation and the launch option.
- The upload's text message `{ "warning" }` existed only for the microphone; it is removed from `offscreen.js`, `protocol.rs`, `server.rs` and `docs/technical/browser-link.md` together. No other message changes. The hello stays `{ token, mime }`.

### Documents to change

- `design-capture-pipeline`: responsibilities (tracks instead of mixing; microphone-only plan), a new invariant for the fixed track layout with silence, `CaptureBackend` in CONS-001, conformance.
- `design-browser-tab-recording`: RESP-002 without the microphone, new responsibility and invariants for the microphone recording and merge, PROV-001 (`start_capture` returns the start instant), failure responsibilities for the merge.
- `docs/technical/browser-link.md`: the offscreen document's role, the hello's timing, no warning message.
- `PRODUCT.md`, `ARCHITECTURE.md`, `README.md` and the usage pages wherever they describe what a file contains or the extension's microphone.

### Units, in dependency order

1. **Two tracks in the pipeline.** Plan roles, per-track blocks, `write_audio` with a track, writers with two inputs. Acceptance: fake-backend tests show track 1 carrying only the app audio, track 2 only the microphone, and silence on a track whose source failed to open; the macOS writer test finishes a file that AVFoundation reads with two audio tracks; on a Mac a self-test file has two audio tracks (ffprobe). The Windows writer needs a self-test on Windows.
2. **Microphone-only recording** (needs 1). Plan without video, writers without video, `MicrophoneRecorder` adapter. Acceptance: a fake-backend test writes audio and no video for the plan; the macOS writer finishes an audio-only file.
3. **Merge** (needs 2 for its input format). Pure placement, merge procedure, shared MP4 remuxer. Acceptance: placement tests (earlier, later, equal starts; cut at the tab's end); procedure tests with fakes (success replaces and removes the microphone file; remux failure keeps both and removes the temporary file); remuxer tests on a fragmented MP4 built in the test (fragments copied byte for byte, priming and placement applied, cut at the end, non-fragmented and truncated input refused); a manual merge of a real browser recording keeps every packet and timestamp of its tracks (ffprobe).
4. **Tab orchestration** (needs 2, 3). `LinkedRecorder` start, stop and failure order; `start_capture` returns the instant. Acceptance: fake tests for the order, for a failed tab start removing the microphone file, for a failed microphone start still recording the tab, and for a tab failure that still merges.
5. **Extension** (independent; lands with 4 so that recordings do not lose the microphone in between). Remove microphone capture and options UI, send hello after start, remove the warning message on both sides. Acceptance: protocol and server tests no longer accept a warning; a manual Google Meet call on a Mac gives a file with two audio tracks, the user's voice only on track 2.
6. **Documents** (with each unit). `docs lint` passes and the design documents state the contracts above.

### Open questions

- Whether a recording whose microphone or other side is turned off in `config.toml` should keep the silent track (as planned, so the layout is fixed) or drop it. Planned: keep it.
