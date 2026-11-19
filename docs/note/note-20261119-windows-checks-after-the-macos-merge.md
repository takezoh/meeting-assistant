---
id: note-20261119-windows-checks-after-the-macos-merge
kind: note
title: Windows checks after the macOS merge, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations:
- {type: references, target: note-20261119-windows-handoff-after-the-macos-work-of-}
- {type: references, target: issue-20261119-tab-recordings-on-windows-will-keep-the-}
- {type: references, target: note-20261119-windows-idle-and-recording-resources}
source_paths:
- crates/app/src/recording/mp4.rs
- crates/app/src/platform/win/recorder/mod.rs
- crates/app/src/platform/win/recorder/writer.rs
summary: The Windows writer's microphone recording added to a recording in place after
  reading Media Foundation's fragments, the self-test's two audio tracks, metadata
  written into Media Foundation's output, a small picture centered, and idle use with
  the merged build, on one PC.
---

## Summary

Observed on one PC on 2026-11-19 after main was fast-forwarded to 6900d1b: Windows 11 (build 26200) with an NVIDIA GeForce RTX 4060 Ti, used over Remote Desktop, with no default microphone. These are the first runs on Windows of the macOS work of 2026-11-18 and 2026-11-19, following [the Windows handoff](note-20261119-windows-handoff-after-the-macos-work-of-.md).

The tests, clippy, the extension's tests and the five ignored recorder tests passed with the merged code. As the handoff expected, the microphone recorded by the Windows writer could not have been added to a tab recording: Media Foundation's fragments were not read. With that fixed, a microphone recording made by the Windows writer was added to a recording made by it as its last track, and both decoded in place. The self-test wrote the video and the two audio tracks in the designed order, with the sound played only in the other side's track; writing metadata into Media Foundation's output left every sample unchanged; a picture smaller than the canvas was drawn in its middle at its own size; and the merged build used no measurable CPU while idle.

## Notes

### Run first

- `cargo test --workspace`, `cargo clippy --workspace --all-targets` and `cargo fmt --all --check`: passed, 234 app tests with 7 ignored. `node --test "extension/test/*.test.mjs"`: 10 passed. `docs lint`: no warnings.
- The five ignored recorder tests: passed. The display was refreshing again by then, so a capture asked for a picture each millisecond sent 64 in 2 seconds against 5 when asked for one each 400 ms.

### What Media Foundation writes

Two files written by `Mp4Writer` from generated sound: one with video and two AAC tracks, and one with a single AAC track and no video, as the microphone is recorded beside a tab. The sink writer accepted both as fragmented MP4. Their layout, read box by box:

- The `moov` has empty sample tables and an `mvex`; there is no edit list, so no priming is declared.
- Each fragment's track fragment header gives an explicit base data offset counted from the start of the file, and no `tfdt`: a fragment starts where the track's previous one ended. Each run gives every sample's duration, size, flags and composition offset.
- A fragment index (`mfra`) ends the file. In the file without video the single track is track 2, and the index names track 1.

`recording::mp4` took a microphone recording's samples from its sample tables only, so it would have found none in this file, as [the issue](../issue/issue-20261119-tab-recordings-on-windows-will-keep-the-.md) expected. It now also reads them from the fragments. A recording whose fragments use explicit offsets had them left pointing into the grown `moov`, and Media Foundation then found no sound in it; those offsets and the fragment index now move with the `moov`, and the added fragments go before the index. A browser's tab recording addresses its data from each fragment and is not affected.

### A microphone recorded by the Windows writer, added in place

The ignored test `a_microphone_recorded_here_is_added_to_a_recording_where_it_was_heard` writes a recording with video whose other side sounds on the left from 1.5 s, and a microphone recording without video that sounds on the right from 1.0 s, adds the microphone 0.5 s late and decodes the result with Media Foundation:

| Track | First sample | Loud from |
| --- | --- | --- |
| Other side | 0 ms | 1500.1 ms |
| Microphone | 500 ms | 1500.1 ms |

Each track held only its own channel. Media Foundation's encoder and decoder between them added no delay that could be measured to a tenth of a millisecond. No tab recording written by a browser was merged on Windows, so how a browser's AAC priming lines up with Media Foundation's is not known.

### The order of the tracks

In every file above the tracks stood in the designed order: the video as track 1, the other side as track 2, the microphone as track 3, the last. In the merged file the microphone was added as track 3 after the recording's two, and its fragments followed the recording's, before the fragment index. Media Foundation's source reader lists a file's tracks in the reverse of that order, so the test tells the decoded tracks apart by their sound rather than by the order the reader gives.

### Self-test

A 10-second `--self-test` of the release build while a 440 Hz tone played on the left channel only, from 3 seconds after the command started:

- The 1920x1080 display at 1920x1080, encoded by `NVIDIA H.264 Encoder MFT (hardware, takes NV12)`; 172 pictures arrived and 167 were taken, drawn into 2 textures; no frame began late.
- Two AAC tracks of 10.05 s after the video. Track 2, the other side's, held the tone on the left only (peak 8066, the right channel at 2), loud from 2.73 s; track 3, the microphone's, was silent.
- The microphone was not recorded: the log said "there is no default audio device", although an Oculus virtual headset microphone was active. The output device's input reported 2 underruns missing 65 ms in all.

### Metadata written into Media Foundation's output

The ignored test `writes_metadata_into_the_file_named_in_the_environment` wrote four keys into the self-test file. Every sample of its three tracks (301 pictures, 471 and 471 AAC frames) kept its duration and bytes, found through the moved offsets; the keys and values were in the `moov`; Media Foundation decoded the sound as before. ffprobe was not available, so whether ffmpeg reads the keys from this file was not checked.

### A picture smaller than the canvas

The ignored test `a_picture_smaller_than_the_canvas_is_drawn_in_its_middle_at_its_own_size`: a 320x180 picture on a 640x360 canvas covered columns 160 to 479 and rows 90 to 269, and the rest was black.

### Idle

The release build of the merged code, started with a configuration of its own and left 40 seconds without a meeting and without opening the panel, then sampled for 60 seconds: 0.00% of one core and 4.3 MiB private. The figure before the merge was 0.03% ([the idle note](note-20261119-windows-idle-and-recording-resources.md)).

### Not checked

- A microphone of any kind, and the Windows privacy setting for it.
- A browser tab recording on Windows, with the microphone added to it or not.
- Slack, Teams, Zoom or Webex on Windows: their windows, and when they take and let go of the microphone.
