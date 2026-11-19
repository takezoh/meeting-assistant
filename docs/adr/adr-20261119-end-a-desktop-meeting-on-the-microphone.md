---
id: adr-20261119-end-a-desktop-meeting-on-the-microphone
kind: adr
title: End a desktop meeting when its app lets go of the microphone
status: proposed
created: '2026-11-19'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/detector.rs
- crates/core/src/engine.rs
summary: A desktop meeting continues and ends on its app's microphone use alone, as
  it starts; its window decides the picture only.
---

## Context

A desktop meeting starts when its app has used the microphone for the start debounce. Since ee9ea9c it also continued while the window that appeared with it was open, so that an app that lets go of the microphone while muted would not end the recording; the user had asked for that when choosing the end condition ("the microphone or the meeting window, with a 2-second grace").

On one Mac on 2026-11-19 that window kept a Slack huddle's recording going for 17 s after leaving: Slack let go of the microphone at once but hid the huddle window and destroyed it 13 s later, and the hidden window still counted as open ([observation](../note/note-20261119-slack-huddle-window-after-the-merge.md)). Slack kept the microphone while muted in every huddle observed. No app has been seen letting go of the microphone while muted; Zoom, Teams and Webex have not been observed.

## Decision

A desktop meeting continues while its app uses the microphone and ends when it has not for the end grace, the same condition that starts it. Its window decides only what the picture shows. The windows are no longer read just because a desktop meeting is detected; they are read while some process uses the microphone or an app's meeting is recorded.

If an app turns out to let go of the microphone while muted, what keeps its meeting going belongs in that app's adapter ([ADR](adr-20261119-tell-meeting-windows-apart-through-app-adapters.md)), not in a rule for every app.

## Consequences

- Positive: a recording ends within the grace of leaving, whatever an app does with its windows afterwards.
- Positive: start and end use one condition, and a window can no longer keep a meeting alive by mistake.
- Negative: in an app that lets go of the microphone while muted, a mute longer than the end grace ends the recording, and unmuting starts another.

## Confirmation

`crates/core/tests/detector.rs` asserts that a meeting ends after the grace from the microphone alone while its window is still listed, and `crates/core/tests/engine.rs` that the recording stops then without moving to another window of the app.
