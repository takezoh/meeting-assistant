---
id: note-20261119-recording-metadata-verification
kind: note
title: Recording metadata verification, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations: []
source_paths:
- crates/app/src/recording/annotate.rs
- crates/app/src/recording/mp4.rs
- extension/background.js
- crates/app/src/apps/slack.rs
summary: Metadata written into a Slack huddle, two Google Meet calls and two earlier
  recordings on one Mac, read back with ffprobe and AVFoundation.
---

## Summary

On 2026-11-19, on one Mac (macOS 26) with Slack (desktop, English) and Chrome 157, the build that writes recording metadata ([ADR](../adr/adr-20261119-write-meeting-metadata-into-the-recording.md)) was installed and recorded one Slack huddle and two Google Meet calls. The huddle and the second call carried every key; the first call lacked the meeting's own keys because Chrome still ran the previous extension. Two recordings made before were rewritten by hand and every packet stayed the same. Windows was not run.

## Notes

### Recordings of the installed build

| Recording | Keys read by ffprobe and by AVFoundation |
| --- | --- |
| Slack huddle, 19:55:47, 5.7 s | `app` = Slack, `started_at` = 2026-11-19T19:55:47+09:00, `meeting.conversation` = dev-gcp-errors, `meeting.workspace` = tkz |
| Google Meet, 19:56:01, 19.7 s | `app` = GoogleMeet, `started_at` = 2026-11-19T19:56:01+09:00; no meeting keys |
| Google Meet, 19:58:17, 4 s | `app` = GoogleMeet, `started_at` = 2026-11-19T19:58:17+09:00, `meeting.meeting_code` = dfw-ituv-utu, `meeting.page_title` = Meet - dfw-ituv-utu |

All keys are under `com.meeting-assistant.`. AVFoundation read them in the `mdta` key space. `started_at` matched each file's name. The log said "Wrote what is known of the recording into its file." for each, after "Added your own voice to the recording as its second audio track." for the calls. No `.annotating`, `.merging` or `.microphone.m4a` file was left, and ffmpeg decoded every stream.

Before the first call the extension had been installed but, it seems, not reloaded in Chrome: an extension of the previous version sends the meeting's `title`, which the new app no longer reads, so the meeting's metadata was empty. The user then reloaded the extension; the log shows its connection closing at 19:57:59 and 19:58:07, and the next call carried the meeting's keys.

### Rewriting earlier recordings

The ignored test `writes_metadata_into_the_file_named_in_the_environment` rewrote copies of a macOS recording (`moov` last, 1.6 MB) and a merged Google Meet tab recording (fragmented, `moov` first, 5.2 MB). Each grew by 378 bytes. For both, a SHA-256 of every packet's stream, timestamps, duration, size and data hash from `ffprobe -show_packets -show_data_hash sha256` was the same before and after. Decoding the tab recording with ffmpeg printed 33 non-monotonic DTS warnings for its video, the same 33 as the original from Chrome.

### Placement checked before writing

Written by `ffmpeg -movflags use_metadata_tags`, the keys sit in `moov/udta/meta` with version and flags; ffprobe read them by key, AVFoundation only as numbered iTunes items (`itsk` 1 to 6). Moved to a `meta` right in the `moov` without version and flags, both read them by key; with version and flags there, AVFoundation read nothing.
