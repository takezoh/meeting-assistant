---
id: note-20261119-macos-verification-2026-11-19
kind: note
title: macOS verification, 2026-11-19
status: draft
created: '2026-11-19'
topic: verification
summary: First ScreenCaptureKit captures on a Mac. Display capture and the microphone
  worked; output audio was silently dropped until the buffer-list call was fixed;
  every update loses the screen recording permission. Dated observations, not guarantees.
tags: []
owners: []
relations:
- {type: references, target: issue-20261117-macos-has-no-native-window-display-or-ap}
- {type: references, target: issue-20261119-every-update-drops-the-macos-screen-reco}
source_paths:
- crates/app/src/platform/mac/recorder/stream.rs
- crates/app/src/recording/pcm.rs
---

## Summary

On 2026-11-18 evening and 2026-11-19, on one Apple Silicon Mac with macOS 26, release builds were installed with `--install` and ScreenCaptureKit was run for the first time, through `--self-test` started with `open` and through manual screen recordings from the status panel. No real call was made; the macOS capture issue stays open.

These are observations from one machine. They do not establish behavior in real meetings, on Intel Macs or on other macOS versions.

## Notes

### Display and microphone

- With the Screen & System Audio Recording permission, `--self-test` recorded the display at 2880x1800, 30 fps, with output device audio and the microphone. AVFoundation read the file as playable, video and audio 5.07 s, picture not black.
- Two manual screen recordings from the status panel (8.5 s and 25.8 s) were saved on Stop; video and audio had the same length; the user heard the played sound and their own voice in them.

### Output audio was dropped

Both channels of every file were identical, although the sound played was stereo. A test tone with 440 Hz only on the left and 880 Hz only on the right, played with `afplay` and then with QuickTime Player during a self-test, came out on both channels at levels typical of a microphone picking up the speakers (440 Hz at about -47 dB, 880 Hz at about -55 dB). The played sound had reached the file only through the microphone.

A diagnostic build logged what the sound stream delivered: `624 buffers, 0 frames, all silent, 624 not usable (cannot read its samples (-12737))`, format 48000 Hz, 2 channels, 32-bit float, non-interleaved. Core Media's `CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer` returns -12737 (kCMSampleBufferError_ArrayTooSmall) unless the buffer list size is exactly the size it needs (40 bytes for two channels); the code passed room for eight buffers (136 bytes). A unit test with a sample of the same format reproduced it without the permission.

After the fix (ask for the size first, pass exactly that), a self-test with the same tone logged `552 buffers, 529920 frames, loudest -24.0 dBFS` and the file held 440 Hz on the left at -29.5 dB and 880 Hz on the right at -29.5 dB, each about 33 dB above the other side. Each sound stream now logs such a summary when it ends.

### Open observations

- 529,920 frames arrived during a capture of about 5 seconds, about 2.2 times real time at 48 kHz. The file showed no regular clicks that dropping the excess in the mixer would cause. The cause was not investigated.
- Whether a meeting app's helper processes are included in the app-audio filter was not checked; that needs a real call.

### Permission

Every `--install` of a rebuilt binary made the screen recording permission stop working; see [the issue](../issue/issue-20261119-every-update-drops-the-macos-screen-reco.md). Turning the switch off and on was not enough; `tccutil reset ScreenCapture com.meeting-assistant.app` and granting again was.
