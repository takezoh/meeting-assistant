---
id: note-20261117-local-verification
kind: note
title: Local verification on Windows, 2026-11-17
status: published
created: '2026-11-17'
topic: verification
summary: What was run and observed on one Windows 11 PC on 2026-11-17, and what was
  not checked. Dated observations, not guarantees.
tags: []
owners: []
relations: []
source_paths: []
updated: '2026-11-17'
---

## Summary

On 2026-11-17 the app was built and exercised on a single Windows 11 PC. Automated checks passed, and the real capture path, the extension path and installation were run by hand with synthetic meetings. Nothing was checked in a real call on a meeting service, and nothing was run on macOS.

These are observations from one machine on one day. They do not establish behavior on other hardware, other Windows versions or later browser versions.

## Notes

### Automated

- `cargo test --workspace`: 221 tests passed, none failed or ignored.
- `cargo clippy --workspace --all-targets` and `cargo clippy --workspace --target aarch64-apple-darwin`: no warnings.

### Run by hand

| Area | What was done | Observed |
| --- | --- | --- |
| Capture | `--self-test` with a kept file | A playable MP4 with video and audio |
| App scope | A test app with a window and audio, recorded as a detected meeting | The file contained that window and that app's audio |
| Detection | A test process holding the microphone | Recording started after the debounce and ended after the grace period |
| Crash survival | A recording of about 34 seconds, process killed | The file kept 33.9 seconds of video and audio |
| Resource use | Repeated recordings | Leaked capture frames and handlers were found and fixed; no growth afterwards |
| Tab capture without a gesture | Probe extension with `--allowlisted-extension-id` | Succeeded in Edge; the user ran the same probe in Chrome and reported one video and one audio track at 3840x2160 |
| Extension path | A test page holding a WebRTC connection in Edge started from the launcher | The meeting was reported, the tab was recorded and the file was stored |
| Install | `--install`, a second `--install`, `--uninstall` | Files, logon entry, native-host keys, shortcuts and uninstall entry were created and removed; user data stayed |
| Exit | `--quit` and a close request during a recording | The file was finalized before the process ended |

### Not checked

- Real calls on Zoom, Teams, Google Meet, Webex, Slack, Whereby or Jitsi Meet.
- A real logoff or shutdown during a recording. Only the message handling was exercised.
- The appearance of the tray icon, menu and notifications.
- Long recordings, several displays, mixed display scaling, protected content.
- Anything on macOS.
- Behavior after the English translation of user-visible text beyond the automated checks above; the installed copy on this PC was built before that change.
