---
id: adr-20261119-write-meeting-metadata-into-the-recording
kind: adr
title: Write what is known of a meeting into its recording as QuickTime metadata
status: proposed
created: '2026-11-19'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/metadata.rs
- crates/app/src/recording/annotate.rs
- crates/app/src/recording/mp4.rs
- crates/app/src/linked.rs
- extension/background.js
summary: Each app's own metadata, with the app and the recording's start, is written
  into every finished MP4 as QuickTime metadata keys by one shared rewrite.
---

## Context

Recordings are analysed into minutes, so what a recording was of (the app, when it started, which conversation or meeting) should travel with the file. Of what can be had without analysing the screen, the user chose what the app already reads: a Slack huddle window's title (its conversation and workspace) and a Google Meet tab's meeting code and page title. What an app tells differs per app; the user asked that everything but the app's name go to the app as metadata of its own per app, that the app's name and the recording's start be in it too, that it be embedded as the video's metadata, and that the processing be shared.

Three kinds of file are finished: a macOS recording, which AVAssetWriter leaves with its `moov` last; a Windows recording, which Media Foundation writes as a fragmented MP4; and a browser tab recording, a fragmented MP4 with its `moov` first and fragments that address their data from themselves (observed for Chrome on 2026-11-19).

QuickTime metadata keys (`mdta`) carry any reverse-DNS key. Written by `ffmpeg -movflags use_metadata_tags`, they sit in `moov/udta/meta` with version and flags; ffprobe reads them by key there, but AVFoundation reads them only as numbered iTunes items. Moved to a `meta` right in the `moov` without version and flags, both read them by key (checked on one Mac on 2026-11-19).

## Decision

- **One kind of metadata.** `MeetingMetadata` is keys and values in order, kept by the same rules whatever told them (keys of lowercase letters, digits and `_`; values trimmed and cut at 200 characters; at most 8 entries); its first value tells a meeting apart and is shown. An app's adapter reads it from the meeting's window, the browser extension sends it per service in place of the former `title`, and the detector reports what it learns during a meeting, keeping it when its source goes.
- **One file layout.** The file gets `com.meeting-assistant.app` (the app, as the file is named), `com.meeting-assistant.started_at` (the time the file is named after, ISO 8601 with the offset from UTC) and `com.meeting-assistant.meeting.<key>` for each entry, as QuickTime metadata right in the `moov`.
- **One procedure.** `LinkedRecorder`, which every recording goes through, writes the metadata it was last told once a recording is finished or has failed, after a tab recording has its microphone added. It rewrites the file beside itself and puts the result in its place; on failure the recording is left as it was. The rewrite (`recording::mp4::with_metadata`) copies every other byte and moves the offsets that point past the `moov` by as much as it grew: `stco`, `co64`, an explicit base data offset in `tfhd`, and `tfra`.

Rejected:

- Writing metadata through each platform's writer (AVAssetWriter's metadata, Media Foundation attributes) and the browser's: three ways instead of one, Media Foundation has no plain way to set such keys, and what a huddle tells is known only once it is joined, after the writer started.
- A sidecar file beside each recording: the user asked for the metadata to be in the video.
- iTunes-style metadata in `udta`: AVFoundation does not read its keys by name.

## Consequences

- Positive: every MP4 the app finishes carries the same keys, read by key by ffprobe and AVFoundation; the media is untouched (on two real recordings every packet's timing and data were the same before and after).
- Negative: each recording is copied once more when it is finished, and a tab recording twice (the microphone, then the metadata).
- Negative: a WebM tab recording gets no metadata.
- Neutral: Windows recordings have not been rewritten on Windows; the offsets the rewrite moves cover what a fragmented MP4 can hold, but Media Foundation's output has not been looked at.

## Confirmation

`crates/core/src/metadata.rs` tests the rules every source goes through; `crates/core/tests/{detector,coordinator,engine,rules}.rs` that metadata is learned, reported and passed to the recorder with the app and the start; `crates/app/src/recording/mp4.rs` that the metadata lands in the `moov` and that each kind of offset still points at its data; `crates/app/src/recording/annotate.rs` the keys and the replace-or-keep procedure; `crates/app/src/linked.rs` that every finished or failed MP4 recording gets it, a tab after its merge. The ignored test `writes_metadata_into_the_file_named_in_the_environment` in `crates/app/src/adapters` rewrites a real file for checking with other readers.
