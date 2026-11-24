---
id: adr-20261124-write-recordings-in-progress-apart
kind: adr
title: Write recordings in progress in a subfolder and move them into the recordings
  folder when finished
status: proposed
created: '2026-11-24'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/handover.rs
- crates/core/src/coordinator.rs
- crates/core/src/ports.rs
- crates/app/src/adapters/mod.rs
summary: A recording is written in the recordings folder's in-progress subfolder and
  moved into the recordings folder once nothing writes it, so a file there is never
  still being written.
---

## Context

Recordings are turned into minutes by tools that watch the recordings folder. Until now a recording was written there under its final name from its first second: the fragmented MP4 stays playable while it grows, so a file being written could not be told from a finished one by name, size or whether it opens. Finishing also left other files beside it for a while: the microphone recorded beside a tab recording (`<recording>.microphone.m4a`) and the copies written while the microphone or the metadata is added (`<recording>.merging.mp4`, `<recording>.annotating.mp4`). The only sign of a finished file was the metadata written last, which a WebM tab recording does not carry.

The user asked that recordings in progress and finished recordings go to different places.

## Decision

A recording is written in `in-progress`, a folder inside the recordings folder. Once the recorder no longer writes it — after it stopped and after it failed — the coordinator moves its files into the recordings folder:

- The files written beside the recording move first and the recording's own file last, so a file named like a recording appears in the recordings folder only after everything that belongs with it.
- Copies that were replacing the recording are removed when the recording itself is there, since they are unfinished and the recording they came from is intact. Without the recording they are moved, so that nothing is lost.
- The files keep their names unless one is taken in the recordings folder meanwhile; then all of them take the next free `_2`, `_3`, ... together.
- A name is given to a new recording only if it is used neither in the recordings folder nor in progress.
- What a start that failed began is not moved then: a start that did not finish in time may still be preparing, and creating and writing its file, after `start` returned, and the coordinator cannot tell when it ends. It stays in `in-progress`, and its name is not given again.
- At startup, under the single-instance lock and before anything is recorded, what an earlier run left in `in-progress`, failed starts included, is moved the same way and reported.
- The `in-progress` folder is removed when a move leaves it empty, so it is seen only while recording, after a crash, or empty after a start that failed until the next recording is moved.

The folder is inside the recordings folder so that the move is a rename on the same volume, done at once; a temporary folder elsewhere could make it a copy of a gigabyte. The decisions are pure functions in `crates/core/src/handover.rs`; the folder operations are added to the `OutputFolder` port.

A file in the recordings folder is no longer written by the app. This does not mean it is complete: a recording whose finishing failed is moved as it is, as before it stayed in place.

## Consequences

- A tool that watches the recordings folder (not its subfolders) can treat a new recording file as finished, also for WebM.
- "Never lose the file" holds: a failed recording is moved like any other, and a killed run's file is moved at the next start. Until then it is in `in-progress`.
- A recording in progress is not in the folder the user opens; the status panel shows its name.
- Every exit path that finalizes a recording now also moves it; a failure to move leaves the file in `in-progress` and is reported.
- What a failed start began, usually an empty file, waits in `in-progress` until the app starts again. A start failing this way was not seen in meetings recorded by the installed app from 2026-11-18 to 2026-11-24.
