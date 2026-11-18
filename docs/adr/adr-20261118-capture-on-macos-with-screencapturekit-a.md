---
id: adr-20261118-capture-on-macos-with-screencapturekit-a
kind: adr
title: Capture on macOS with ScreenCaptureKit, AVAssetWriter and AVAudioEngine
status: proposed
created: '2026-11-18'
decision_makers:
- assistant
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-record-only-the-meeting-app}
- {type: references, target: adr-20261117-ports-adapters-fakes}
source_paths:
- crates/app/src/platform/mac/recorder/mod.rs
- crates/app/src/platform/mac/recorder/content.rs
- crates/app/src/platform/mac/recorder/stream.rs
- crates/app/src/platform/mac/recorder/writer.rs
- crates/app/src/platform/mac/recorder/microphone.rs
- crates/app/src/platform/mac/signals.rs
- crates/app/src/recording/pcm.rs
summary: The macOS CaptureBackend captures windows, displays and app audio with ScreenCaptureKit, the microphone
  with AVAudioEngine, and writes H.264/AAC MP4 with AVAssetWriter; detection reads Core Audio process objects.
---

# Capture on macOS with ScreenCaptureKit, AVAssetWriter and AVAudioEngine

## Context

On macOS the app recorded browser tabs through the extension only; desktop meeting apps and manual recording were not implemented. The user asked for macOS desktop recording and the microphone, with deciding code kept apart from code that acts. The shared pipeline (`crates/app/src/recording`) already decides what to open, when to write and how to mix; a platform supplies a `CaptureBackend` of small operations. The recording rule in [Record only the meeting app](adr-20261117-record-only-the-meeting-app.md) still applies: when the app's audio cannot be captured, the whole output is not recorded instead.

## Decision

- **Pictures:** ScreenCaptureKit, one stream per target: a window through a desktop-independent window filter, or a display. The stream is asked for pictures at the recording's size in `420v`, scaled to fit with the aspect ratio kept, so the system scales and the encoder takes the pictures without a colour conversion. Only the latest picture is kept; the pipeline takes it at its own frame rate.
- **App audio:** ScreenCaptureKit audio of a filter that includes the meeting app only, at 48 kHz stereo, excluding this app's own sound. **All output audio** (no app known) uses a display filter. ScreenCaptureKit captures what apps play whatever the output device, so the render device in a request is not used on macOS.
- **Microphone:** AVAudioEngine's input node, resampled to 48 kHz in shared pure code (`recording::pcm`). It needs the microphone permission only, which keeps it usable for recordings made by the browser extension later.
- **Writing:** AVAssetWriter, H.264 at the configured quality (not a fixed bitrate) and AAC 128 kb/s, in an MP4 with a movie fragment every 2 seconds.
- **Detection:** Core Audio process objects (whether a process is using input or output and on which device), libproc for the process tree and CGWindowList for windows.

Float audio is converted and resampled by pure functions in `recording::pcm`; the macOS modules only call the OS.

## Alternatives considered

- **Core Audio process taps for app audio (macOS 14.2).** They capture a process's audio without the screen recording permission, but need an aggregate device per recording and their own permission prompt. ScreenCaptureKit is needed for pictures anyway, so its audio adds no permission.
- **AVCaptureScreenInput and CGWindowListCreateImage.** Deprecated or removed in current macOS, and neither captures a single window's content efficiently.
- **A fixed average bitrate.** On 2026-11-18 on one Apple Silicon Mac, two seconds of noise at 640x360 came to 2.9 MB at quality 30 and 23.5 MB at quality 90, so the H.264 encoder honours the quality setting, as on Windows.

## Consequences

- Positive: the shared pipeline, mixer, timeline and their tests are reused; the macOS recorder holds OS calls only, about 1,100 lines with its tests.
- Positive: hardware encoding where the Mac has it, and no colour conversion between capture and encoder.
- Negative: capturing needs the Screen & System Audio Recording permission and the microphone permission, given by the user in System Settings. The app is signed ad hoc, so macOS may ask again after an update.
- Negative: ScreenCaptureKit needs macOS 14 for the size of what a filter captures, and detection needs macOS 14.2 for Core Audio process objects.
- Neutral: whether ScreenCaptureKit's app filter includes audio played by an app's helper processes was not established on 2026-11-18. Zoom and Teams may play through helpers.
