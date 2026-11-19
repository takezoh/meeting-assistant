---
id: adr-20261119-tell-meeting-windows-apart-through-app-adapters
kind: adr
title: Tell meeting windows apart through adapters of the meeting apps
status: proposed
created: '2026-11-19'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/apps.rs
- crates/app/src/apps
- crates/core/src/detector.rs
summary: What is known about a particular meeting app's windows lives in an adapter
  of a core port; the detector names no app and goes by when a window appeared for
  apps without one.
---

## Context

The user set four conditions for the picture of a desktop meeting: the app's main window is never recorded (a Slack window shows other channels); the picture starts only once the meeting's own window is found, black until then; the windows are looked at only once some process uses the microphone, not while the machine is idle; and what is particular to an app is kept behind an abstraction.

Until the merge of 2026-11-19 a meeting's window was the window of its app that appeared from 10 seconds before the microphone was taken, which needs to know which windows were open before. That came from reading the windows on every tick. With the windows read only from the microphone on, a huddle on one Mac was recorded black throughout: Slack opens the huddle window and takes the microphone together, when the huddle is previewed, so at the first reading the main window and the huddle window were both open and their timing told them apart no more ([observation](../note/note-20261119-slack-huddle-window-after-the-merge.md)).

What tells an app's windows apart once the microphone is taken differs per app, so no one condition serves every app. For Slack it is a mark: Slack adds "[Main]" to its main window's title while a huddle it has joined is open in another window. The "Huddle in …" header of the huddle window is drawn inside the app and is not in the title the system reports.

## Decision

The core has a port, `MeetingApp`, through which an adapter for one app says what each of the app's windows is (its main window, a meeting's, or nothing known) from the titles read at one time. `RuleEvaluator` gives each rule the adapter that handles one of its process names, and the evidence carries each window's role. The detector names no app: a window told as a meeting's can be the meeting's window from the first reading, one told as the main window never is, and of windows told nothing, only one that appeared after the windows began to be read is, as before. Adapters live in `crates/app/src/apps`, are pure, and are given to the engine where it is composed; the first is Slack's, which tells the window with "[Main]" in its title as the main one and the others as the huddle's.

The windows are still read only while some process uses the microphone or a desktop meeting is detected or recorded.

Rejected:

- Reading the window identifiers every few seconds while idle, to keep the history that told the windows apart before: the user rejected looking at the windows while no microphone is in use.
- One condition for every app, such as when a window appeared: it cannot tell Slack's huddle window from its main window.
- The app's mark as a setting of its rule: tried and observed in one huddle, then replaced by an adapter at the user's request, so that what is known about an app is code behind a port rather than text users copy into their settings.
- The order in which windows were created: macOS window numbers rise as windows are created, but Windows has nothing equivalent.
- Recording the main window when no meeting window is found: it records what the user did not want recorded.

## Consequences

- Positive: a Slack huddle window is recorded from the moment the huddle is joined, although it opened before the windows were first read, and the main window is not (observed once on one Mac, with the mark given as a rule setting).
- Positive: supporting another app is an adapter and its tests; the detector and the configuration do not change.
- Negative: a huddle's preview before joining is black, since Slack does not mark its main window then.
- Negative: the mark is Slack's wording and may change with Slack or its language; when it is not found the picture is black, never the main window.
- Neutral: Zoom, Teams and Webex have no adapter; whether their call windows appear after the microphone is taken has not been observed.

## Confirmation

`crates/core/tests/detector.rs`, `crates/core/tests/engine.rs` and `crates/core/tests/rules.rs` check, with the fake adapter, that a window told as a meeting's is recorded although it was open at the first reading, that a window told as the main one never is, and that windows told nothing go by when they appeared. `crates/app/src/apps/slack.rs` checks the Slack adapter against the titles observed. `crates/app/src/architecture.rs` keeps `apps` free of side effects.
