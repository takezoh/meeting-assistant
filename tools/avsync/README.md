# avsync

Measures how far sound and picture are apart in a recording of the screen, on Windows, and lists what a recording holds. It was written to check the Windows recording path; the results are in the [measurement note](../../docs/note/note-20261118-windows-recording-measured.md).

It is not part of the app's workspace and is built and run by hand:

```
cargo build --release --manifest-path tools/avsync/Cargo.toml
```

## Measure

1. Start the stimulus. Once per period it turns a 160-pixel square in the top-left corner of the primary display from black to white and plays a 60 ms tone of 1 kHz on the default output device at the same moment:

   ```
   tools\avsync\target\release\avsync.exe stimulus 620 2.0
   ```

2. While it runs, record the display with the app, for example `meeting-assistant --config <dev config> --self-test out.mp4 600`. The self-test records the primary display and all output audio, so it captures both.

3. Decode the file and pair each flash with its tone:

   ```
   tools\avsync\target\release\avsync.exe analyze out.mp4
   ```

   It prints each pair as `time_s,offset_ms`, where a positive offset means the sound comes later than the picture, then a summary and the median for each minute.

## What the numbers include

- The stimulus itself: drawing the square reaches the screen a display refresh or so after the tone is queued, and the tone waits about 15 ms in the output buffer. On the PC it was written on, a recording whose log showed 179 ms of sound queued at the start measured 146 ms, so the stimulus makes sound look about 30 ms earlier than it is. Compare runs with each other rather than reading one offset as absolute.
- Frames: the picture side is quantized to the frame interval (33 ms at 30 fps). The stimulus and the recording run on the same clock, so the flash falls at the same point of a frame interval every time, and even the medians sit at one of two values a frame apart, switching between them now and then. A difference of less than a frame between two runs cannot be read; a change over time within one run can.
- The square and the tone are seen and heard on the PC while it runs.

## Inspect a recording

```
toolsvsync	argeteleasevsync.exe tracks out.mp4
```

It prints the top-level boxes, each track in the order the file stores it and the QuickTime metadata keys. It then decodes every track with Media Foundation:

- for each audio channel, its peak, its mean level and when it first goes above -40 dBFS;
- for the video, where in the picture something other than black is, each time that changes. A window recorded at its own size shows as a box in the middle, a black picture as none.

Media Foundation lists a file's streams in another order than its tracks, and its stream identifiers are not the track IDs. Each stream is therefore matched to the track whose first sample sizes it reads. A video stream whose samples Media Foundation changes is matched as the only video track. This tells the other side's track from the microphone's by position, without relying on what each one sounds like.
