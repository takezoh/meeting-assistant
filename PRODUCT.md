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
| Do not record the gaps | A recording ends when microphone use has stopped for `end_grace_seconds` |
| Record the meeting, not the desktop | Video is one window or one tab; the other side's audio is the audio of that app or tab |
| Keep recording while you work elsewhere | A tab recording continues while another tab is visible; a window recording continues while the window is covered |
| Never lose the file | Fragmented MP4 is written continuously; exit, logoff and shutdown finalize it |
| Decide for yourself when it matters | Manual start and stop, and a pause for automatic recording, in the tray menu |
| Adapt to your apps | Rules in `config.toml` name processes and window-title patterns; `--probe` shows how each rule evaluates right now |

## How a meeting is detected

Once per `poll_interval_milliseconds` the app reads which processes hold an active microphone session and which windows exist, and evaluates the rules.

- **Start.** A rule's process, or one of its descendants, uses the microphone continuously for `start_debounce_seconds` (3 by default). A rule with `require_window_title` also needs a window of that process whose title matches one of its patterns; this is how a browser using the microphone is told apart from a browser in a meeting.
- **Continue.** The meeting lasts while that process keeps using the microphone. A browser title that changes because another tab was selected does not end it.
- **End.** Microphone use has been absent for `end_grace_seconds` (15 by default).

With the browser extension connected, a browser meeting is detected differently: the extension reports when a page on a supported meeting site holds a connected WebRTC call. This also detects a call you only listen to. A browser whose extension is connected is excluded from window-based detection so that one meeting is not recorded twice.

The default rules cover Zoom, Microsoft Teams, Webex and Slack as desktop apps, and Chrome, Edge, Firefox, Brave, Vivaldi, Opera and Arc with title patterns for Google Meet, Microsoft Teams, Zoom, Webex, Slack, Whereby and Jitsi Meet. The extension supports Chrome and Edge and the sites listed in `extension/manifest.json`.

## What is recorded

With the default `capture_scope = "app"`:

| | Desktop app, or browser without the extension | Browser with the extension |
| --- | --- | --- |
| Video | One window. For a title-matched rule, the window whose title matched; otherwise the largest window of the app | The meeting tab |
| Other side's audio | Audio played by the app's top-level process and its descendants | Audio of the tab |
| Your audio | The microphone | The microphone |

- The picture size is fixed when the recording starts. If the window is resized, the picture is scaled to fit with its aspect ratio kept and the rest filled with black.
- If the recorded window closes, recording moves to another window of the same app. If there is none, the last picture stays and audio continues.
- If no window of the app is found at the start, the recording starts black and moves to the window when it appears. The app does not substitute the whole display.
- If the app's audio cannot be captured, the recording continues without it. The app does not substitute all output audio.
- A manual recording started while no meeting is detected has no target app, so it records the primary display and all output audio.

With `capture_scope = "screen"` the display that holds the meeting window and all audio of the output device are recorded.

The two "does not substitute" rules are deliberate: a person who chose to record only the meeting should not find other windows or other apps' audio in the file. See [the decision](docs/adr/adr-20261117-record-only-the-meeting-app.md).

## Control and lifecycle

- The tray menu starts and stops a recording, pauses and resumes automatic recording, toggles start at logon, and opens the recordings folder, the configuration file and the log folder.
- A manual recording is not ended by meeting detection; it ends when you stop it or the app exits.
- Quitting from the menu, `--quit`, logoff and shutdown finalize a recording before the process ends.
- If a recording fails mid-way, the app keeps the partial file, reports the failure and tries again while the meeting continues.
- Changes to `config.toml` take effect the next time the app starts. An invalid configuration is reported at startup instead of being partly applied.

## What Meeting Assistant does not promise

- It does not tell other participants that they are being recorded, and it does not check consent, policy or law for you.
- It does not detect a meeting in which the microphone is never used, unless the extension detects the call.
- It cannot tell a meeting from another use of the microphone by the same app, such as a voice message or a device test.
- It does not respect the meeting's mute button. Your microphone is captured directly.
- Without the extension it does not record a browser tab. It records the browser window, so switching tabs in that window changes what is recorded, and audio from every tab of that browser is included.
- It does not record a shared screen or a second window that the meeting app opens separately from the window it chose.
- It does not update the picture while the recorded window is minimized.
- It does not transcribe, summarize, upload, encrypt or delete recordings. Files stay where they were written until you remove them.
- It does not run as a Windows service or without a logged-in user, because capture needs the user's session.

## What is not established

- Detection has been checked with synthetic meetings (a test process holding the microphone, and a test page holding a WebRTC connection), not in real calls on each service. Whether a given service releases the microphone while you are muted, and so ends the recording after the grace period, is unknown.
- The browser switch that permits automatic tab recording, `--allowlisted-extension-id`, is undocumented and intended for testing. It worked in Chrome and Edge on 2026-11-17. A later browser version may remove it; recording then does not start and the log says why.
- All macOS code has only been type-checked on Windows. Nothing has been run on a Mac.
- The tray menu and notifications were not inspected by an automated test; their logic is tested, their appearance is not.
- Long recordings, several displays with different scaling, and protected content have not been measured.

[The verification note](docs/note/note-20261117-local-verification.md) records what was observed and on which date. Open gaps are tracked under [docs/issue](docs/issue/).

## Scope

In scope: detection by rules, per-app and per-tab recording, manual control, per-user installation and start at logon, graceful finalization.

Out of scope: transcription and summaries, cloud storage, scheduling from a calendar, recording on behalf of other users, mobile platforms, and recording that hides itself from the user. The tray icon always shows when a recording is in progress.

[Introduction](README.md) · [Install](INSTALL.md) · [Architecture](ARCHITECTURE.md) · [Development](docs/development.md)
