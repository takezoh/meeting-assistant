---
id: note-20261119-windows-idle-and-recording-resources
kind: note
title: Windows resource use while idle and while recording, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-measured}
- {type: references, target: note-20261119-windows-encoder-and-1080p}
source_paths:
- crates/core/src/engine.rs
- crates/app/src/panel/window.rs
- crates/app/src/platform/win/recorder/video.rs
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/recorder/writer.rs
- crates/app/src/platform/win/recorder/audio.rs
summary: Idle CPU after reading fewer signals, the status panel's memory after its
  page is let go, the capture rate limit, the encoder's texture pool, event-driven
  audio capture, and a 30-second recording compared with the previous build.
---

## Summary

Observed on one PC on 2026-11-19: Windows 11 (build 26200), an NVIDIA GeForce RTX 4060 Ti, used over Remote Desktop. During the measurements the remote session did not refresh the display: a display capture received one picture in ten seconds while a window on it changed every 10 ms, and a click posted to the tray icon did not reach the app. Windows captured on their own did refresh, so capture of the display under load was not measured.

With no meeting, the resident app used 1.28% of one core before it read fewer signals and 0.03% after. Thirty seconds after the status panel closes its page is let go, and the panel's web view processes, about 166 MiB of private memory, end. In a 30-second self-test against the previous build, GPU use fell from 8.0% to 4.2% and the encoder's from 28.9% to 8.0%; that recording repeated one picture throughout.

## Notes

### Signals read while idle

The installed build of commit 9a51e2f and a development build that reads the playback sessions, the process tree and the windows only while an app uses the microphone (or an app's meeting is recorded) were each left running with no meeting and `poll_interval_milliseconds = 1000`, and sampled for 60 seconds:

| Build | The app's process | With the panel's web view processes |
| --- | --- | --- |
| 9a51e2f | 1.28% of one core, 5.6 MiB private | 1.30%, 181.7 MiB |
| Fewer signals read | 0.03%, 6.3 MiB | 0.10%, 172.3 MiB |

### The status panel's page

The web view's processes stayed at about 172 MiB after the panel had been shown and hidden, with the web view's memory level set to low while hidden. Letting the page go 30 seconds after the panel closes ended them: 40 seconds after start, sampled for 20 seconds, the app was alone at 0.08% of one core and 4.5 MiB private.

Opening the panel after that loads the page again. Opened from inside the app (a temporary call to `show`, since the posted tray click did not arrive), the page was made 185 ms after `show` and reported ready 16 ms later, and the panel became visible. A screenshot of the reopened panel was black, as was every capture of that session's display at the time, so what it drew was not seen.

### Pictures asked of the capture

`GraphicsCaptureSession::SetMinUpdateInterval` is set to the frame interval. The ignored test `capture_sends_no_more_pictures_than_one_per_interval` resizes a window every 4 ms for 2 seconds: the capture sent 21 pictures when asked for one per millisecond and 5 when asked for one per 400 ms. The remote session composed at about 10 pictures a second, so the limit at the default 30 fps could not be told apart from that rate here.

### Textures the encoder reads

The ignored test `pictures_are_drawn_into_a_texture_again_only_once_the_encoder_has_let_go_of_it` makes 24 pictures in four colors, writes each three times, decodes the file and checks the color of all 72 frames, with the NVIDIA encoder and with Windows' software encoder:

| Writing | Textures used for 24 pictures |
| --- | --- |
| As fast as possible | 24 (the encoder held every sample) |
| One frame each 33 ms, as when recording | 2 |

Every frame had its picture's color, and all textures were let go of after finalizing. With the check for held textures removed, the NVIDIA encoder's file showed the last picture's color from the second frame on: the hardware encoder reads a texture after `WriteSample` has returned.

### Audio capture woken by Windows

Capture now waits for the event WASAPI signals instead of waking every 10 ms. The ignored test `sound_played_is_captured_from_the_output_device_and_from_the_process_that_plays_it` plays a 440 Hz tone for 2 seconds: the output device's loopback captured 1.92 s of it and the process loopback 1.99 s, both at the played level. No microphone was connected, so the microphone path was not run.

### A 30-second self-test against the previous build

`measure.py` ran a 30-second `--self-test` of the 1920x1080 display with the A/V stimulus playing, once with the installed build of 9a51e2f and once with the development build. One picture arrived in each, so the video was one picture repeated; the stimulus's beeps were captured from the output device.

| | 9a51e2f | Development build |
| --- | --- | --- |
| Encoder input | ARGB32 | NV12 |
| GPU use, mean of 32 samples | 8.0% | 4.2% |
| Encoder use | 28.9% | 8.0% |
| GPU memory in use (whole GPU) | 3307 MiB | 3216 MiB |
| CPU | 4.1% of one core | 3.6% |
| Writing a frame, mean and slowest | 0.08 ms, 2 ms | 0.03 ms, 0 ms |
| Audio | 30.2 s, 180 frames repeated to follow the clock | 30.2 s, 270 frames dropped |

The mixer moved each recording's audio by a few milliseconds (180 or 270 frames of 48 kHz) to bring the amount queued to its target; with sound arriving as soon as Windows has it, it started a little higher. The 20 ms margin of the mixer was not changed: lowering it needs a long recording of real sound, which this session could not provide.
