---
id: note-20261118-macos-verification-2026-11-18
kind: note
title: macOS verification, 2026-11-18
status: published
created: '2026-11-18'
topic: verification
summary: What was built, installed and run on one Mac on 2026-11-18, and what was
  not checked. Dated observations, not guarantees.
tags: []
owners: []
relations: []
source_paths:
- crates/app/src/platform/mac/mod.rs
- crates/app/src/platform/mac/install.rs
- crates/app/src/tray.rs
updated: '2026-11-18'
---

## Summary

On 2026-11-18 the app was built natively and exercised on one Apple Silicon Mac (macOS 26.6.2) with Rust 1.100.0, Chrome 157.0.8103.80 and Edge 157.0.4341.60. This was the first time the macOS code was linked and run. The installer, start at login, the extension link and tab recording, and finalization on quit, update, uninstall and a terminate request were observed to work. Meetings were synthetic; nothing was checked in a real call.

These are observations from one machine on one day. They do not establish behavior on Intel Macs, other macOS versions or later browser versions.

## Notes

### Automated

- `cargo test --workspace`: all tests passed on macOS.
- `cargo clippy --workspace --all-targets` on macOS and `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc` from macOS: no warnings. The Windows target was type-checked only.
- `cargo fmt --all --check`: clean.

### Changed as a result of running on a Mac

| Found | Change |
| --- | --- |
| The installed app was a bare executable in `~/Library/Application Support`, which Finder, Launchpad and Spotlight do not list | The app is installed as `~/Applications/Meeting Assistant.app` with an `Info.plist` (`LSUIElement`) |
| The installer started the app as its own child, tied to the terminal it ran in | The installed app is started through LaunchServices (`open -g`) |
| Logout, restart and a quit sent by another app end a macOS app through the application's terminate request, not a signal; the tray ignored the last event of its loop, so a recording was not finalized on that path | The tray finalizes the recording on the event loop's last event, once |
| Whether there was a command-line output was guessed from the parent process, so the tray's Install item reported nothing | Output exists when standard output is a terminal, file, pipe or socket |
| Rust 1.100 deprecates `AtomicUsize::fetch_update` | Replaced with a compare-exchange loop |

### Run by hand

| Area | What was done | Observed |
| --- | --- | --- |
| CLI | `--probe`, `--self-test`, an unknown argument, `--quit` with nothing running | `--probe` lists no processes or windows (not implemented on macOS); `--self-test` fails saying only tab recording is supported; usage and exit codes as documented |
| Install | `--install` from the build folder | `~/Applications/Meeting Assistant.app`, the extension, the LaunchAgent, native-host manifests for Chrome and Edge and two launcher apps were written; every plist passed `plutil -lint`; the app started with launchd as parent, standard I/O on `/dev/null`, and was registered as `com.meeting-assistant.app` of type UIElement |
| Start at login | `launchctl bootstrap gui/<uid>` of the installed LaunchAgent, as login does | launchd started the app; it connected to the extension and recorded |
| Extension path | A throwaway Chrome profile with the installed extension, `--allowlisted-extension-id`, and a page on `https://meet.jit.si/` served locally through DevTools request interception holding a loopback WebRTC call | The relay connected, the call was detected after about 3 seconds, the tab was recorded and the file was saved 16 seconds after hanging up (end grace 15 seconds) |
| Recording | Four finished files inspected with AVFoundation | Each was playable with an H.264 video track at 2560x1426 and an AAC audio track |
| Update | `--install` during a recording | The running app saved the file and exited, the new copy started, the extension reconnected within 30 seconds and the ongoing call was recorded to a new file |
| Quit | `--quit` during a recording | The file was saved before the process exited |
| Uninstall | `--uninstall` during a recording | The file was saved; the app, launchers, LaunchAgent, native-host manifests and extension folder were removed; configuration, logs and recordings stayed |
| Terminate request | `osascript -e 'tell application id "com.meeting-assistant.app" to quit'`, run by the user during a recording of about 7.6 minutes. This sends the quit Apple event that logout also sends; it is not a logout | The log showed "The application is being terminated.", then the file was saved and the process exited. AVFoundation read the file as playable, 456 seconds long |

### Observations to keep in mind

- The browser switch `--use-fake-ui-for-media-stream` makes every tab capture fail with "Requested device not found". Automated browser tests must not use it; grant the microphone another way.
- After uninstall, the LaunchAgent job that launchd loaded at login stays loaded, without a program, until logout. It does not start anything.
- The microphone was not granted to the extension in the test profile, so the recordings contain tab audio only and the app logged the documented warning. Mixing the microphone into a tab recording was not observed on macOS.
- A failed first capture attempt blocked the retries for that tab; see [the issue](../issue/issue-20261118-a-failed-tab-capture-start-blocks-retrie.md).
- In every file the video track was shorter than the audio track: 31.9 against 32.8 seconds, and 441.9 against 456.3 seconds in the long recording. The test page changed its picture ten times a second and its window was not kept in front. The cause was not investigated.

### Status panel, later the same day

A development build was run with its own configuration while the installed copy was stopped. A small script stood in for the extension: it connected to the app's loopback link with the published token, reported meetings in tabs and answered `start` and `stop`. The user looked at the screen; this session could not capture it.

| What was done | Observed |
| --- | --- |
| Opened the panel from the menu bar icon | The panel hung below the icon; the menu bar showed the running time next to the red icon |
| Two meetings reported, one after the other | Both were listed; the first was recorded, the second shown as waiting; one recording at a time |
| Stop on the recorded meeting (first build) | The recording stopped and the other meeting was recorded automatically, while the stopped one was mislabelled as waiting. Both were changed: a stop by hand now starts nothing, and the label was fixed |
| The quit button | The first build quit at once; it now asks first. The panel was too translucent to read over a terminal; its background was made nearly opaque |
| Meeting ended while recorded automatically, another waiting | The recording was finalized and the waiting meeting was recorded next |
| Two Google Meet tabs with meeting codes; Stop on the recorded one | The rows were told apart by their codes, the menu's first line named both, and stopping started no other recording |

The stand-in reported WebM, which exposed that notices and the panel name such a file `.mp4`; see [the issue](../issue/issue-20261118-a-tab-recording-saved-as-webm-is-reporte.md). The installed copy was started again through its LaunchAgent afterwards and reconnected to the browser extension. The installed copy and its extension were not updated.

### Not checked

- A real logout, restart or shutdown during a recording. Only the quit Apple event and SIGTERM were exercised.
- Starting the browser from the launcher apps, and loading the extension through the browser's extensions page in a normal profile.
- The appearance of notifications and dialogs. The menu bar icon and the status panel only as described above, with synthetic meetings.
- Recording through Edge. Edge was started only for the capture experiments that used the fake-UI switch.
- Real calls on any meeting service, long recordings, Intel Macs.
