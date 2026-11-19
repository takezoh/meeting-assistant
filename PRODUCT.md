# Meeting Assistant — Product

Meeting Assistant is a resident desktop app that records your meetings for you. It detects that a meeting has started in a desktop app or a browser, records only that meeting, and finalizes the file when the meeting ends.

This page defines what the product does, for whom, and where its behavior stops. [README](README.md) introduces it, [INSTALL.md](INSTALL.md) sets it up, and [Architecture](ARCHITECTURE.md) explains how it is built.

## Outcome and acceptance

Success means a person finds a usable recording of each meeting they attended without having started, stopped or framed it, and the recording contains the meeting rather than the rest of their desktop.

Detecting a meeting, opening a capture and writing a file are intermediate capabilities. Acceptance also requires that the recording starts close to the start of the call, survives the ways a session really ends (leaving, closing the window, logging off, a crash), and does not silently include what the person did not intend to record.

## Who it is for

| Audience | Why it fits |
| --- | --- |
| People who attend many calls and want a record to return to | Recording does not depend on remembering to press a button in each meeting app |
| People who use several meeting services | One recorder and one folder regardless of the service, including services without their own recording |
| People who share a screen with other work on it | The default records the meeting app or tab, not the display |

It is a personal tool for one logged-in user on one computer. It has no accounts, no upload, no transcription and no shared storage.

## Value and approach

| Desired benefit | Supporting behavior |
| --- | --- |
| Never forget to record | The app starts at logon and starts a recording when a rule's process has used the microphone for `start_debounce_seconds` |
| Do not record the gaps | A recording ends `end_grace_seconds` after the app has stopped using the microphone and closed its call window |
| Record the meeting, not the desktop | Video is one window or one tab; the other side's audio is the audio of that app or tab |
| Keep recording while you work elsewhere | A tab recording continues while another tab is visible; a window recording continues while the window is covered |
| Never lose the file | Fragmented MP4 is written continuously; exit, logoff and shutdown finalize it |
| Decide for yourself when it matters | Manual start and stop, and a pause for automatic recording, in the tray menu |
| Adapt to your apps | Rules in `config.toml` name the processes of desktop meeting apps; `--probe` shows how each rule evaluates right now |

## How a meeting is detected

Desktop meeting apps are detected from their use of the microphone; browser meetings are detected by the browser extension, and only by it. Once per `poll_interval_milliseconds` the app reads which processes hold an active microphone session and evaluates the rules; it looks at the windows only while an app uses the microphone or a desktop meeting is recorded.

- **Start.** A rule's process, or one of its descendants, uses the microphone continuously for `start_debounce_seconds` (3 by default). The meeting's window is found from what can be seen once the app has taken the microphone. For an app the app knows more about, that is what the app shows: Slack marks its main window while a joined huddle is open, so the huddle window is the meeting's from the moment of joining. For other apps it is a window that appears after the app took the microphone, or up to 10 seconds before while the windows were already being looked at, such as a Zoom meeting window. Either way it may be found later during the meeting, and the app's main window is never the meeting's.
- **Continue.** The meeting lasts while the app keeps using the microphone, the same condition that started it. Its window decides only what the picture shows.
- **End.** The app has not used the microphone for `end_grace_seconds` (2 by default), whatever windows it still has. In an app that lets go of the microphone while muted, a longer mute would end the recording; Slack keeps the microphone while muted.

Each meeting is followed on its own, so several can be detected at once, for example a Zoom call and a Google Meet tab, or two Meet tabs; two meetings of one service are told apart by the Google Meet meeting code or the tab's title. Only one recording exists at a time: a meeting that starts while nothing is recorded is recorded automatically, and when that meeting ends, a meeting that started meanwhile is recorded next.

A browser meeting is detected when the extension reports that a page on a supported meeting site holds a connected WebRTC call, with its own shorter timing: it starts after `browser_start_debounce_seconds` (1 by default) and ends after `browser_end_grace_seconds` (1 by default) without a call. A connected call does not drop out while you are muted, and a call that loses its connection briefly counts as connected while the browser tries to restore it; only a call that is closed or has failed ends the meeting. This also detects a call you only listen to. Without the extension, a meeting in a browser is not detected; a browser that uses the microphone is not taken for a meeting.

The default rules cover Zoom, Microsoft Teams, Webex and Slack as desktop apps. The extension supports Chrome and Edge and the sites listed in `extension/manifest.json`.

## What is recorded

With the default `capture_scope = "app"`:

| | Desktop app | Browser meeting (extension) |
| --- | --- | --- |
| Video | One window: the meeting's window; black until it is found | The meeting tab |
| Other side's audio | Audio played by the app's top-level process and its descendants | Audio of the tab |
| Your audio | The microphone | The microphone, recorded by the app beside the tab |

Every recording has two audio tracks in this order: the other side's audio first, your microphone second. They are not mixed, so that a recording turned into minutes keeps who said what. A source that is turned off in the configuration or cannot be captured leaves its track silent; the order does not change.

For a browser meeting the extension records the tab, and the app records the microphone beside it in a file named `<recording>.microphone.m4a`. When the meeting ends, the app adds the microphone to the tab's file as its second audio track without re-encoding either, and removes the separate file. If that fails, or the browser recorded WebM, the tab's file stays as the browser wrote it and the microphone file stays beside it; the log says so.

- The picture is `video.width` by `video.height`, 1920x1080 by default and at most. A larger window or display is scaled down to fit with its aspect ratio kept and the rest filled with black, also when the window changes size or shape during the recording. On Windows a smaller one is recorded at its own size in the middle; on macOS ScreenCaptureKit fits it to the picture.
- Video is encoded by the GPU's H.264 encoder, whichever vendor made the GPU, and by Windows' software encoder where there is none.
- If the recorded window closes while the meeting goes on, the last picture stays and audio continues; a window that appears later during the meeting is recorded from then on.
- If no window has appeared with the meeting when it starts, the recording starts black and moves to the meeting's window when it appears. The app does not substitute its main window or the whole display.
- If the app's audio cannot be captured, the recording continues without it. The app does not substitute all output audio.
- A manual recording of a detected meeting is recorded the same way as its automatic recording. Recording the entire screen is a separate, explicit choice; it records the primary display and all output audio.

With `capture_scope = "screen"` the display that holds the meeting window and all audio of the output device are recorded.

The two "does not substitute" rules are deliberate: a person who chose to record only the meeting should not find other windows or other apps' audio in the file. See [the decision](docs/adr/adr-20261117-record-only-the-meeting-app.md).

## Control and lifecycle

- The tray icon shows the state: a ring while idle, amber when a meeting is detected but not recorded, red while recording. The idle ring follows the menu bar or the taskbar, dark on a light one and light on a dark one. While recording, the macOS menu bar shows how long the recording has been running next to the icon; on Windows the icon's tooltip shows it.
- Clicking the icon opens a status panel in the platform's own style: the translucent popover material on macOS, the acrylic material, controls and accent color of Windows 11 flyouts on Windows. It shows the recording with its running time and file name, every detected meeting with whether it is recorded, waiting or not recorded, and a Record or Stop button for each, a switch for automatic recording, and recording the entire screen. The menu (right click) keeps the same actions and also toggles start at logon.
- A manual recording is not ended by meeting detection; it ends when you stop it or the app exits.
- Stopping a recording by hand starts no other recording, and that meeting is not recorded automatically again. Switching automatic recording off and on again clears this.
- Quitting from the menu, `--quit`, logoff and shutdown finalize a recording before the process ends.
- If a recording fails mid-way, the app keeps the partial file, reports the failure and tries again while the meeting continues.
- Changes to `config.toml` take effect the next time the app starts. An invalid configuration is reported at startup instead of being partly applied. A file from a version of the app that wrote settings in another format is set aside and replaced with the defaults, and the app says so; older settings are not converted.

## What Meeting Assistant does not promise

- It does not tell other participants that they are being recorded, and it does not check consent, policy or law for you.
- It does not detect a meeting in which the microphone is never used, unless the extension detects the call.
- It cannot tell a meeting from another use of the microphone by the same app, such as a voice message or a device test.
- It does not respect the meeting's mute button. Your microphone is captured directly.
- It does not detect or record a meeting in a browser without the extension, or in a browser the extension does not support.
- It does not record a shared screen or a second window that the meeting app opens separately from the window it chose.
- It does not update the picture while the recorded window is minimized.
- It does not transcribe, summarize, upload, encrypt or delete recordings. Files stay where they were written until you remove them.
- It does not run as a Windows service or without a logged-in user, because capture needs the user's session.

## What is not established

- Detection has been checked with synthetic meetings (a test process holding the microphone, and a test page holding a WebRTC connection), and on 2026-11-19 with Slack huddles on one Mac: Slack kept the microphone while muted, its huddle window was a separate window that it hid as the huddle was left and destroyed 0.45 to 13 seconds later, and it let go of the microphone at once. Zoom, Teams and Webex have not been observed: whether they release the microphone while muted, which would end the recording during a mute longer than the end grace, is unknown.
- The browser switch that permits automatic tab recording, `--allowlisted-extension-id`, is undocumented and intended for testing. It worked in Chrome and Edge on 2026-11-17. A later browser version may remove it; recording then does not start and the log says why.
- On macOS the installer, start at login, the extension path and finalization on quit, update and uninstall were run on one Mac with synthetic meetings on 2026-11-18. A real logout during a recording, the browser launchers and the menu bar appearance were not checked.
- On macOS, recording the display with all output audio and the microphone was observed on one Mac on 2026-11-19, through the self-test and manual screen recordings, after a defect that dropped all output audio was fixed. Recording a meeting app's window and audio has not been observed, nor whether an app's audio includes what its helper processes play. Each update of the app currently makes macOS stop honouring its Screen & System Audio Recording permission until it is granted again; see [the issue](docs/issue/issue-20261119-every-update-drops-the-macos-screen-reco.md).
- The microphone recorded by the app and added to a browser tab recording was observed once, on one Mac on 2026-11-19, in a 15-second Google Meet call without other participants: the voice was in the second audio track and the tab's own tracks were copied unchanged. How well the microphone lines up with the other side's audio has not been measured. On Windows the two audio tracks and this merge have only been type-checked.
- The tray menu, the status panel and notifications were not inspected by an automated test; their logic is tested, their appearance is not. On one Mac on 2026-11-18 the panel, the menu bar icon and its running time were looked at by the user, with a stand-in for the extension reporting synthetic meetings. On one Windows 11 PC on 2026-11-18 the panel was opened from the tray and captured with no meeting detected; its other states were only rendered from sample content in Edge, and the tray icon, its tooltip and the menu were not looked at.
- The status panel needs the system web view. On Windows this is the WebView2 runtime; without it only the menu is available.
- On one Windows PC on 2026-11-18 a 60-minute self-test of a 3840x2160 display kept the sound where it started against the picture for the whole hour, without a gap; it recorded the output audio only, as there was no microphone ([the measurement note](docs/note/note-20261118-windows-recording-measured.md)). Long recordings with a microphone or of a meeting app, several displays with different scaling, and protected content have not been measured.

[The verification note](docs/note/note-20261117-local-verification.md) records what was observed and on which date. Open gaps are tracked under [docs/issue](docs/issue/).

## Scope

In scope: detection by rules, per-app and per-tab recording, manual control, per-user installation and start at logon, graceful finalization.

Out of scope: transcription and summaries, cloud storage, scheduling from a calendar, recording on behalf of other users, mobile platforms, and recording that hides itself from the user. The tray icon always shows when a recording is in progress.

[Introduction](README.md) · [Install](INSTALL.md) · [Architecture](ARCHITECTURE.md) · [Development](docs/development.md)
