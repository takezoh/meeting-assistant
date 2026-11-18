# Install Meeting Assistant

This page takes you from a checkout to a running app that records desktop meetings, and then through the optional browser extension for per-tab recording. Everything is installed for the current user only; no step needs administrator rights.

## Requirements

- Windows 11. The app was developed and checked on Windows 11 only. Capturing the audio of a single app relies on per-process audio loopback, which older Windows 10 builds do not provide.
- A Rust toolchain that supports edition 2024 (Rust 1.85 or later). No prebuilt binaries are published.
- For per-tab recording: Google Chrome or Microsoft Edge.

For macOS see [macOS](#macos) first; most of the app is not implemented there.

## Build and install

```
cargo build --release
target\release\meeting-assistant.exe --install
```

The executable is its own installer. `--install`:

| Step | Where |
| --- | --- |
| Copies the app and writes out the extension | `%LOCALAPPDATA%\Programs\MeetingAssistant`, extension in its `extension` folder |
| Adds a Start Menu entry | `Meeting Assistant` |
| Starts the app at logon | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| Registers the app as the extension's native-messaging host | `HKCU\Software\Google\Chrome\NativeMessagingHosts` and the Edge equivalent |
| Creates browser shortcuts for the browsers that are installed | Start Menu and Desktop, for example `Google Chrome (Meeting Assistant)` |
| Registers an uninstall entry | Settings > Apps > Installed apps |
| Starts the app | A tray icon appears |

If you start the executable without installing it, the tray menu offers the same installation.

The app does not run as a Windows service. Capturing windows and audio needs the logged-in user's session, so it runs as an ordinary process of that user.

## Check that recording works

```
meeting-assistant --self-test recording.mp4
```

This records five seconds of the primary display and the default audio devices and reports the result. Play `recording.mp4` to confirm picture and sound. Without a path the test file is removed afterwards.

Then join a test call in a desktop meeting app. The tray icon should turn red a few seconds after the app starts using the microphone, and a file should appear in `Videos\MeetingAssistant` shortly after you leave. If not, see [Troubleshooting](docs/usage/troubleshooting.md).

## Browser extension

Without the extension a browser meeting is recorded as a browser window. With it, the meeting tab alone is recorded, and a call is detected when it connects rather than when the microphone is used.

1. Open `chrome://extensions` (or `edge://extensions`), turn on **Developer mode**, choose **Load unpacked** and select `%LOCALAPPDATA%\Programs\MeetingAssistant\extension`.
2. The extension's options page opens. Allow the microphone there, so that your own voice is recorded.
3. Quit the browser completely, including background processes, and start it from the shortcut the installer created (`Google Chrome (Meeting Assistant)` or `Microsoft Edge (Meeting Assistant)`). Keep using that shortcut.

The shortcut starts the browser with:

```
--allowlisted-extension-id=lnplioaidiniokndppmckdaflgefgale
```

A browser normally lets an extension capture a tab only after the user invokes the extension, for example by clicking its button. This switch lifts that requirement for this one extension, which is what makes automatic recording possible. Things to know:

- The switch is undocumented and meant for testing. A future browser version may remove it. Recording then does not start, and the reason is written to the log.
- It has no effect on a browser that is already running, which is why step 3 asks you to quit first.
- Started any other way, the browser still reports meetings, but the tab recording cannot start.
- The extension ID is fixed by the key in `extension/manifest.json`, so the unpacked extension has this ID on every machine.

The extension is active on Google Meet, Microsoft Teams, Zoom, Webex, Slack, Whereby and Jitsi Meet; the exact URL patterns are in `extension/manifest.json`.

## Where things are kept

| Item | Location |
| --- | --- |
| Recordings | `Videos\MeetingAssistant\<date>_<time>_<rule>.mp4` |
| Configuration | `%APPDATA%\MeetingAssistant\config.toml`, created with defaults on first start |
| Logs | `%LOCALAPPDATA%\MeetingAssistant\logs` |
| Extension link endpoint | `%LOCALAPPDATA%\MeetingAssistant\browser-link.json`, rewritten at each start |

`--config <path>` uses another configuration file and writes logs to a `logs` folder beside it. See [Configuration](docs/usage/configuration.md).

## Update

Build the new version and run its `--install` again. The installer asks the running app to finalize any recording and exit, replaces the files and starts the new version. If the running app does not exit, nothing is changed.

After an update that changes the extension, press **Reload** for the extension on the browser's extensions page.

## Uninstall

Use Settings > Apps, or run:

```
meeting-assistant --uninstall
```

This stops the app, removes the logon entry, the native-host registrations, the shortcuts, the uninstall entry and the installed files. Your configuration, logs and recordings are kept; delete those folders yourself if you want them gone. Remove the extension on the browser's extensions page.

## Commands

```
meeting-assistant                      Run in the tray and record detected meetings
meeting-assistant --install            Install or update
meeting-assistant --uninstall          Uninstall
meeting-assistant --quit               Ask the running app to finalize its recording and exit
meeting-assistant --probe              Print the current signals and how each rule evaluates
meeting-assistant --self-test [<path>] Check that a five-second recording can be made
meeting-assistant --config <path> ...  Use another configuration file
```

## macOS

Only meetings detected and recorded by the browser extension are implemented. The tab is recorded inside the browser, so the app only has to receive and store the data. Desktop apps, browser windows without the extension and manual recording need a native capture implementation (ScreenCaptureKit) that does not exist yet; `--self-test` and **Start recording now** report that.

The macOS build needs Rust 1.89 or later and the Xcode Command Line Tools. It was built, installed and run on one Apple Silicon Mac with macOS 26 on 2026-11-18, with synthetic meetings only; [the verification note](docs/note/note-20261118-macos-verification-2026-11-18.md) lists what was and was not observed.

```
cargo build --release
target/release/meeting-assistant --install
```

| Step | Where |
| --- | --- |
| Installs the app | `~/Applications/Meeting Assistant.app`, listed by Finder, Launchpad and Spotlight. It lives in the menu bar and has no Dock icon |
| Writes out the extension | `~/Library/Application Support/MeetingAssistant/extension` |
| Starts the app at login | `~/Library/LaunchAgents/com.meeting-assistant.agent.plist`. macOS may notify you that a background item was added |
| Registers the native-messaging host | The `NativeMessagingHosts` folder of Chrome and Edge, for each browser that has been used on this Mac |
| Creates browser launchers | `~/Applications/Google Chrome (Meeting Assistant).app` and the Edge equivalent |
| Starts the app | A circle appears in the menu bar |

To start the app again after quitting it, open **Meeting Assistant** from Spotlight or `~/Applications`.

Recordings go to `~/Movies/MeetingAssistant`, the configuration to `~/Library/Application Support/MeetingAssistant/config.toml`, and logs to `logs` in the same folder.

The extension steps are the same as on Windows. In the **Load unpacked** dialog, `~/Library` is hidden; press Command-Shift-G and paste `~/Library/Application Support/MeetingAssistant/extension`. Quit the browser completely (Command-Q) before opening its launcher from `~/Applications`.

`--uninstall` removes the app, the launchers, the login item and the native-host registrations, and keeps the configuration, logs and recordings.
