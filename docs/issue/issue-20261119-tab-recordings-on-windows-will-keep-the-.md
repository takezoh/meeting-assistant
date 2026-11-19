---
id: issue-20261119-tab-recordings-on-windows-will-keep-the-
kind: issue
title: Tab recordings on Windows will keep the microphone beside them
status: done
created: '2026-11-19'
issue_type: bug
tags: []
owners: []
relations:
- {type: references, target: design-browser-tab-recording}
subject_paths:
- crates/app/src/recording/mp4.rs
- crates/app/src/platform/win/recorder/writer.rs
summary: The merge reads the microphone recording's samples from its moov sample tables,
  but the Windows writer produces fragmented MP4, so on Windows the microphone is
  expected never to be added. Read from the source; not run.
disposition_target: note-20261119-windows-checks-after-the-macos-merge
updated: '2026-11-19'
---

## Symptom

Expected, not observed: on Windows every browser tab recording is left as the browser wrote it, with `<recording>.microphone.m4a` beside it, and the log says the microphone could not be added because it "does not overlap the recording".

## Evidence

Read in the source on 2026-11-19, commit ee9ea9c; nothing was run on Windows.

- The Windows writer sets `MF_TRANSCODE_CONTAINERTYPE` to `MFTranscodeContainerType_FMPEG4` (`crates/app/src/platform/win/recorder/writer.rs`, `Mp4Writer::create`), so the microphone recording, made by the same writer on the microphone-only plan, is a fragmented MP4 whose samples are described in `moof` boxes.
- `recording::mp4::Source::parse` takes the samples from the audio track's `stbl` tables only (`sample_table`). In a fragmented file those tables are empty, so no sample is placed and `add_audio_track` returns "the microphone recording does not overlap the recording".
- On macOS the same path works because AVAssetWriter rewrites its fragments into an ordinary MP4 when it finishes.

## Resolution

Since 2026-11-19 `recording::mp4` reads a microphone recording's samples from its fragments as well as its sample tables, starting each fragment where the one before ended when it has no `tfdt`. A microphone recording made by the Windows writer was added to a recording made by it and decoded in place; see [the note](../note/note-20261119-windows-checks-after-the-macos-merge.md). No browser's tab recording has had a microphone added on Windows yet.

## Context for follow-up

The merge contract is in [the browser tab recording design](../design/design-browser-tab-recording.md) (INV-008). Either `recording::mp4` reads the samples of a fragmented source from its `moof`/`trun` boxes (with `tfdt` as each fragment's start), or the microphone-only recording is written as an ordinary MP4. The tests in `recording/mp4.rs` build their files in memory and can cover a fragmented source on any OS.


{% transition from="open" to="done" date="2026-11-19" %}
Fixed on 2026-11-19: recording::mp4 reads a fragmented microphone recording. A microphone recording made by the Windows writer was added to a recording and decoded in place.
{% /transition %}
