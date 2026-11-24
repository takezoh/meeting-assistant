---
id: note-20261124-macos-verification-2026-11-24
kind: note
title: 'macOS verification on 2026-11-24: recordings in progress apart, missing sounds
  and a lasting signature'
status: draft
created: '2026-11-24'
tags: []
owners: []
relations:
- {type: references, target: adr-20261124-write-recordings-in-progress-apart}
- {type: references, target: adr-20261124-sign-the-macos-app-with-a-lasting-identity}
source_paths:
- crates/core/src/handover.rs
- crates/core/src/coordinator.rs
- crates/app/src/platform/mac/recorder/microphone.rs
- crates/app/src/install/mod.rs
- crates/app/src/platform/mac/install.rs
summary: What was run on one Mac on 2026-11-24 for the in-progress folder, the missing-sound
  warnings and signing the app with a lasting identity, what was found and fixed,
  and what was not run.
---

## Summary

Observed on 2026-11-24 on one Apple Silicon Mac with macOS 26.6.2, by the user and Claude Code.
- The first part ran on the change that writes recordings in progress apart ([ADR](../adr/adr-20261124-write-recordings-in-progress-apart.md)) and warns about a sound a recording does not have.
- The second part ran on signing the app with a lasting identity ([ADR](../adr/adr-20261124-sign-the-macos-app-with-a-lasting-identity.md)).

`cargo fmt --check`, clippy (all targets, also for `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`), `cargo test --workspace`, the extension's tests and `docs lint` passed before each build was run. Windows was not run.

## How the builds were started

- **From Terminal.** A release build was started from Claude Code's shell, with a configuration that wrote to a test folder.
  - macOS attributed its permission requests to Terminal (`responsible=com.apple.Terminal` in the TCC log).
  - Screen capture was refused at once, so a Slack huddle it detected was recorded with the other side's track silent and no picture.
  - The microphone request waited on a dialog for Terminal: from 14:57:49 until the user allowed it at 14:58:31. Two starts timed out after 20 s each. Both abandoned threads created their files at 14:58:31.757, after the coordinator had already tried to move them.
  - A build started this way is not a valid test of capture.
- **As a separately named bundle.** The build was copied into `Meeting Assistant Dev.app`, with the bundle identifier `com.meeting-assistant.dev`, signed ad hoc, and started with `launchctl bootstrap` from a plist that passed `--config`.
  - macOS attributed every request to `com.meeting-assistant.dev`, and the installed app kept its permissions.
  - When the user chose "Quit & Reopen" after granting screen recording, macOS reopened the bundle without its arguments, so it ran with the default configuration for about 30 s until it was stopped.
  - Every rebuild of that bundle needed its screen recording permission reset and granted again.

## Recordings in progress apart

With the separately named bundle and a test recordings folder:

- **Startup recovery.** Three files had been put in `in-progress` beforehand: a recording, its `.annotating.mp4` copy and an empty `.mp4`.
  - At startup the recording was moved, byte for byte, and the empty file was moved too.
  - The copy was removed, and the folder was removed once empty.
  - Two notices were raised.
- **Recording and stopping.** Two entire-screen recordings started from the panel were written in `in-progress` and moved into the recordings folder when stopped.
- **Killed mid-recording.** A third recording was killed with `kill -9` after 64 s.
  - Its file stayed in `in-progress` and was playable.
  - At the next start it was moved, with a notice.
- **Failed starts.** Three starts that failed because screen recording was not allowed left no file, but left the `in-progress` folder empty behind them.

Before the fix in 125832b, the coordinator moved what a failed start left as soon as `start` returned. The two Terminal-run timeouts above left empty files in `in-progress` that way.

## Missing sounds

- **A microphone denied in the microphone dialog.** On macOS such a microphone still opens through AVAudioEngine and delivers silence. The first build therefore neither logged nor warned, and its microphone track measured -91 dB. The fix reads AVCaptureDevice's authorization status after the engine starts.
- **The rebuilt bundle with its microphone denied.** A rebuild of the bundle had its microphone turned off in System Settings (the change was logged at 17:53:51).
  - `--self-test`, started through launchd, logged `microphone will not be recorded`.
  - It printed `Warning: the microphone track stays silent: macOS does not let Meeting Assistant use the microphone...`.
  - It recorded the microphone track as silence.
  - Allowed, it gave no warning.
- **The installed app with its microphone denied.** At 18:52:35 the user answered "Don't Allow" to the microphone dialog after the identity changed.
  - Three recordings, two from the panel and one Google Meet tab, logged the same warning at their start.
  - The tab recording's microphone track measured -91 dB, and its tab sound -14.3 dB.
  - Once the user allowed the microphone at 19:03:50, a self-test's microphone track measured a mean of -51.7 dB.
- **Not seen.** Whether the warning notification appeared on screen was not confirmed. Neither was a failure to open the other side's sound with a known meeting.

## A lasting signature

- **The identity.** A self-signed code-signing certificate, `Meeting Assistant Local Signing`, was created in the login keychain from the command line. `security find-identity -p codesigning` listed it as `CSSMERR_TP_NOT_TRUSTED`, and `codesign` signed with it nonetheless. The app's designated requirement became `identifier "com.meeting-assistant.app" and certificate leaf = H"d2cfdf94…"`.
- **Before the change.** An install of an ad-hoc build had left the screen recording permission shown as on but refused. Switching it off and on did not help.
- **Installing the debug build.**
  - The installer reported the signature and that the earlier permissions were cleared.
  - The TCC log shows the screen recording entry deleted at 18:52:09.
  - The user then granted screen recording, and the microphone as described above.
- **Installing the release build**, a different binary. The installer reported the signature and no change of it.
  - A self-test recorded the display, and the microphone with a mean of -51.0 dB, and asked for nothing.
  - The TCC log shows no prompt and no change for `com.meeting-assistant.app` after the update.
- **Not established.**
  - Whether `codesign` asked the user before using the key.
  - Creating the identity through Keychain Access, as INSTALL.md describes.

## Cleanup

The test bundle's permissions were reset with `tccutil reset All com.meeting-assistant.dev`. Its folder, holding the test recordings and a microphone-only recording of the Terminal-run huddle, was moved to the Trash. Terminal kept the microphone permission granted at 14:58:31.
