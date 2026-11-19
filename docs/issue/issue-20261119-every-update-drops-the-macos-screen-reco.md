---
id: issue-20261119-every-update-drops-the-macos-screen-reco
kind: issue
title: Every update drops the macOS screen recording permission
status: open
created: '2026-11-19'
issue_type: bug
tags: []
owners: []
relations:
- {type: references, target: note-20261119-macos-verification-2026-11-19}
subject_paths:
- crates/app/src/platform/mac/install.rs
summary: After each --install of a new build, macOS treats Meeting Assistant as not allowed to record the screen and
  system audio until the user removes and grants the permission again; the app is only linker-signed ad hoc.
---

## Symptom

After installing a new build, every capture fails with "The user declined TCCs for application, window, display capture", although System Settings still lists Meeting Assistant as allowed. Turning the switch off and on again did not help; only resetting the app's entry and granting it again did. A user would have to do this after every update, and until then desktop meetings and manual recordings fail.

## Evidence

On one Mac (macOS 26) on 2026-11-19, see [the verification note](../note/note-20261119-macos-verification-2026-11-19.md):

- 2026-11-18 23:35 a self-test succeeded with the permission granted. After `--install` of a rebuilt binary, the self-test at 01:23:52 failed with the TCC error above.
- After the user switched the permission off and on, a self-test at 01:25:56 failed the same way.
- `tccutil reset ScreenCapture com.meeting-assistant.app`, then a self-test to make macOS ask, then granting it: the self-test succeeded. The same was needed again after the next `--install`.
- `codesign -dv` on the installed bundle: `Identifier=meeting_assistant`, `Signature=adhoc`, `flags=0x20002(adhoc,linker-signed)`, no team identifier. The bundle itself is not signed; only the linker's ad-hoc signature of the executable exists, whose hash changes with every build.

## Context for follow-up

The installer is in `crates/app/src/platform/mac/install.rs` and the decision to install a self-copied executable is in [the self-installing ADR](../adr/adr-20261117-self-installing-per-user-executable.md). The Microphone permission is affected too: after the `--install` of 10:51 on 2026-11-19, macOS asked for the microphone again when the next browser tab recording started, and that recording waited for the answer, so the first seconds of the call were not recorded ([verification note](../note/note-20261119-two-audio-tracks-verification-2026-11-19.md)).
