# Meeting Assistant

## Your meetings are recorded without you remembering to press record

Meeting Assistant sits in the notification area, notices when a meeting starts in a desktop app or a browser, records it, and stops when the meeting ends. The recording contains the meeting and nothing else on your screen: the meeting window or browser tab, the audio of that app, and your microphone.

> A recording contains the video and audio of the other participants. Obtaining their consent and following your organization's rules and the law that applies to you is your responsibility. Meeting Assistant does not announce the recording to anyone.

Windows is supported. On macOS desktop apps and manual recording are implemented but have not yet recorded a real window or display on a Mac; see [Platform support](#platform-support).

## Join a meeting; the file appears when it ends

Join a call in Zoom, Microsoft Teams, Webex or Slack, or open Google Meet in Chrome or Edge. A few seconds after the call is live the tray icon turns red. When you leave, the recording is finalized as an MP4 (H.264 video up to 1080p, AAC audio) in `Videos\MeetingAssistant`, named after the start time and the rule that matched, for example `2026-11-17_14-00-05_Zoom.mp4`.

The tray icon shows the state: a ring while idle, amber when a meeting is detected but not being recorded, red while recording. Click it to see every detected meeting and the recording's running time, and to record or stop each meeting, in a panel that looks like the system's own: a popover on macOS, a Windows 11 flyout on Windows.

## Only the meeting is recorded

| | What is recorded by default |
| --- | --- |
| Video | The meeting window, or the meeting tab when the browser extension is connected |
| Their voices | The audio that app or tab plays, not every sound on the PC |
| Your voice | The microphone |

Other windows, notifications and audio from other apps stay out of the file. If the meeting window cannot be found, the recording continues with a black picture and the meeting audio instead of falling back to the whole screen. Set `capture_scope = "screen"` when you do want the whole display and all output audio.

Their voices and yours are kept apart, in two audio tracks: the first holds the meeting, the second your microphone. A tool that turns recordings into minutes can then tell who spoke; a player that plays only the first audio track plays only the meeting.

Your microphone is captured directly by the app, also for a browser meeting, so your voice is recorded even while you are muted inside the meeting.

## Browser meetings are recorded per tab

With the bundled Chrome/Edge extension, a browser meeting is detected when the call connects in the page, including when you only listen, and only that tab is recorded, even while you look at another tab. Browser meetings are detected only through the extension.

Automatic tab recording needs the browser to be started with an undocumented switch. The installer creates a browser shortcut that adds it; see [INSTALL.md](INSTALL.md#browser-extension) for the steps and the caveat.

## Stay in control

The status panel that opens from the tray icon lists the meetings in progress and starts or stops the recording of each by hand, pauses automatic recording, and opens the recordings folder, the configuration file and the log folder. The tray menu offers the same actions. Rules decide which apps and which browser window titles count as a meeting; edit them in `config.toml` and check them with `meeting-assistant --probe`. See [Configuration](docs/usage/configuration.md).

Logging off, shutting down, quitting from the menu or running `meeting-assistant --quit` finalizes the file first. If the app is killed while recording, the file remains playable up to a moment before the kill.

## Install

```
cargo build --release
target\release\meeting-assistant.exe --install
```

The executable is its own installer. It changes only the current user's profile and needs no administrator rights: it copies itself to `%LOCALAPPDATA%\Programs\MeetingAssistant`, starts at logon, registers itself with Chrome and Edge for the extension, and adds an entry under Settings > Apps. `--uninstall` reverts this and keeps your configuration, logs and recordings.

On macOS the same two commands install `~/Applications/Meeting Assistant.app`; see [INSTALL.md](INSTALL.md#macos).

[INSTALL.md](INSTALL.md) covers the browser extension, updating, macOS and where files are kept.

## Platform support

| | Windows | macOS |
| --- | --- | --- |
| Browser meetings with the extension, per tab | Supported | Supported; run with synthetic meetings on one Mac |
| Desktop apps such as Zoom | Supported | Implemented; screen and app-audio capture not yet run on a Mac |
| Manual recording | Supported | Implemented; not yet run on a Mac |
| Install, start at login, finalize on exit, browser launcher | Supported | Supported; the launcher and a real logout were not run |

## What to expect

- A desktop-app meeting you join without ever using the microphone is not detected, and neither is a browser meeting without the extension. Record the entire screen from the status panel instead.
- Detection has been exercised with synthetic meetings on one PC, not with every meeting service. [Product](PRODUCT.md#what-is-not-established) lists what is and is not established.
- [Troubleshooting](docs/usage/troubleshooting.md) covers a meeting that was not recorded, missing audio and a black picture.

## Develop

- [Product behavior and limits](PRODUCT.md)
- [Architecture and responsibility boundaries](ARCHITECTURE.md)
- [Development and verification](docs/development.md)
- [Decisions and their reasons](docs/adr/)

## License

The Cargo metadata declares the MIT license. A `LICENSE` file has not been added to the repository yet.
