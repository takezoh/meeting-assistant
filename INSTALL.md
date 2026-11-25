# Install Meeting Assistant

This page takes you to a running app that records desktop meetings, and then through the optional browser extension for per-tab recording. On Windows you install from the executable; on macOS, from a disk image. Everything is installed for the current user only; no step needs administrator rights.

## Requirements

- Windows 11. The app was developed and checked on Windows 11 only. Capturing the audio of a single app relies on per-process audio loopback, which older Windows 10 builds do not provide.
- For per-tab recording: Google Chrome or Microsoft Edge.
- To build the app yourself: a Rust toolchain that supports edition 2024 (Rust 1.85 or later), with the MSVC toolchain on Windows. No builds are published.

For macOS see [macOS](#macos) instead of the next section.

## Install on Windows

1. Open `Meeting-Assistant-<version>.exe`. If Windows says "Windows protected your PC", which SmartScreen says of a downloaded program that is not signed, choose **More info**, then **Run anyway**.
2. Meeting Assistant asks whether to install itself. Choose **OK**. It reports what it did, and a ring appears in the notification area.
3. You may delete the file you opened. The installed app is in `%LOCALAPPDATA%\Programs\MeetingAssistant`.

Choosing **Cancel** changes nothing. The executable is made by `packaging\windows\build.ps1`; see [the packaging reference](docs/technical/packaging.md). No executable is published, so get it from whoever builds them, or build it as below.

| Step | Where |
| --- | --- |
| Copies the app as `meeting-assistant.exe` and writes out the extension | `%LOCALAPPDATA%\Programs\MeetingAssistant`, extension in its `extension` folder |
| Adds a Start Menu entry | `Meeting Assistant` |
| Starts the app at logon | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| Registers the app as the extension's native-messaging host | `HKCU\Software\Google\Chrome\NativeMessagingHosts` and the Edge equivalent |
| Creates browser shortcuts for the browsers that are installed | Start Menu and Desktop, for example `Google Chrome (Meeting Assistant)` |
| Registers an uninstall entry | Settings > Apps > Installed apps |
| Starts the app | A tray icon appears |

The app does not run as a Windows service. Capturing windows and audio needs the logged-in user's session, so it runs as an ordinary process of that user.

### Install from a checkout

```
cargo build --release
target\release\meeting-assistant.exe --install
```

`--install` takes the same steps without asking. Opened without a command, `target\release\meeting-assistant.exe` asks as the downloaded file does.

## Check that recording works

```
meeting-assistant --self-test recording.mp4
```

This records five seconds of the primary display and the default audio devices and reports the result. Play `recording.mp4` to confirm picture and sound. Without a path the test file is removed afterwards.

Then join a test call in a desktop meeting app. The tray icon should turn red a few seconds after the app starts using the microphone, and a file should appear in `Videos\MeetingAssistant` shortly after you leave. If not, see [Troubleshooting](docs/usage/troubleshooting.md).

## Browser extension

Browser meetings are detected and recorded only through the extension. The meeting tab alone is recorded, and a call is detected when it connects rather than when the microphone is used.

1. Open `chrome://extensions` (or `edge://extensions`), turn on **Developer mode**, choose **Load unpacked** and select `%LOCALAPPDATA%\Programs\MeetingAssistant\extension`.
2. The extension's options page opens and shows the launch option the browser needs. The extension does not use the microphone: the app records your voice itself and adds it to the tab's recording.
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

On Windows, open the newer executable and choose **OK**, or run its `--install`. On macOS, open the newer disk image, open the app in it and choose **Install**. The installer asks the running app to finalize any recording and exit, replaces the files and starts the new version. If the running app does not exit, nothing is changed. On macOS see also [Keeping the permissions across updates](#keeping-the-permissions-across-updates).

After an update that changes the extension, press **Reload** for the extension on the browser's extensions page.

If the new version stores its settings in another format, it keeps your old `config.toml` beside the new one as `config.toml.unversioned.bak` or `config.toml.v<version>.bak` and starts with the defaults; a notification says so. Copy back the settings you need; see [Configuration](docs/usage/configuration.md).

## Uninstall

Use Settings > Apps, or run:

```
meeting-assistant --uninstall
```

This stops the app, removes the logon entry, the native-host registrations, the shortcuts, the uninstall entry and the installed files. Your configuration, logs and recordings are kept; delete those folders yourself if you want them gone. Remove the extension on the browser's extensions page.

## Commands

```
meeting-assistant                      Run in the tray and record detected meetings; opened from elsewhere
                                       than the install location, ask whether to install instead
meeting-assistant --install            Install or update
meeting-assistant --uninstall          Uninstall
meeting-assistant --quit               Ask the running app to finalize its recording and exit
meeting-assistant --probe              Print the current signals and how each rule evaluates
meeting-assistant --self-test [<path> [<seconds>]]
                                       Check that a recording can be made, five seconds unless <seconds> is given
meeting-assistant --config <path> ...  Use another configuration file
```

## macOS

Desktop meeting apps, manual recording and the browser extension are implemented. macOS 14.2 or later on a Mac with Apple silicon is needed: detection reads which apps use the microphone from Core Audio, and ScreenCaptureKit captures windows, displays and app audio.

### Install from the disk image

1. Open `Meeting-Assistant-<version>.dmg`.
2. Open **Meeting Assistant** in it. If macOS says it cannot verify the app, which it does for a disk image downloaded from the web, open System Settings > Privacy & Security and choose **Open Anyway** for Meeting Assistant. Then open it again.
3. Meeting Assistant asks whether to install itself. Choose **Install**. It reports what it did, and a circle appears in the menu bar.
4. Eject the disk image. Do not keep or copy the app from it anywhere else; the installed app is in `~/Applications`.

Choosing **Cancel** changes nothing. The disk image is made by `packaging/macos/build.sh`; see [the packaging reference](docs/technical/packaging.md). No disk image is published, so get it from whoever builds them.

| Step | Where |
| --- | --- |
| Installs the app | `~/Applications/Meeting Assistant.app`, listed by Finder, Launchpad and Spotlight. It lives in the menu bar and has no Dock icon |
| Writes out the extension | `~/Library/Application Support/MeetingAssistant/extension` |
| Starts the app at login | `~/Library/LaunchAgents/com.meeting-assistant.agent.plist`. macOS may notify you that a background item was added |
| Registers the native-messaging host | The `NativeMessagingHosts` folder of Chrome and Edge, for each browser that has been used on this Mac |
| Creates browser launchers | `~/Applications/Google Chrome (Meeting Assistant).app` and the Edge equivalent, with the browser's own icon |
| Starts the app | A circle appears in the menu bar |

To start the app again after quitting it, open **Meeting Assistant** from Spotlight or `~/Applications`.

Recordings go to `~/Movies/MeetingAssistant`, the configuration to `~/Library/Application Support/MeetingAssistant/config.toml`, and logs to `logs` in the same folder.

The extension steps are the same as on Windows. In the **Load unpacked** dialog, `~/Library` is hidden; press Command-Shift-G and paste `~/Library/Application Support/MeetingAssistant/extension`. Quit the browser completely (Command-Q) before opening its launcher from `~/Applications`.

### Permissions

Recording a desktop app or the screen needs two permissions, which macOS asks for the first time the app records:

| Permission | Where | Without it |
| --- | --- | --- |
| Screen & System Audio Recording | System Settings > Privacy & Security. Turn on **Meeting Assistant**, then quit and start it again | Recordings of desktop apps and of the screen fail to start; browser tabs are recorded by the extension and are not affected |
| Microphone | System Settings > Privacy & Security > Microphone | The microphone track of every recording, browser tabs included, is silent |

### Keeping the permissions across updates

macOS ties both permissions to the app's signature. The disk image's app is signed when it is built, and an update keeps both permissions as long as it was signed by the same identity as the version it replaces; the installer says who signed it. When the signature changes, the installer clears the old Screen & System Audio Recording permission so that macOS asks for it again, and macOS asks for the microphone again; give both again. This happens once when you first install from a disk image whose signer differs from that of the installed app, and after every update to an app signed ad hoc, which the installer warns about. See [the decision](docs/adr/adr-20261124-distribute-a-built-app-that-installs-itself.md).

### Uninstall

There is no uninstall entry on macOS yet ([the issue](docs/issue/issue-20261125-uninstalling-on-macos-needs-terminal.md)). In Terminal run:

```
~/Applications/Meeting\ Assistant.app/Contents/MacOS/meeting-assistant --uninstall
```

This removes the app, the launchers, the login item and the native-host registrations, and keeps the configuration, logs and recordings. Moving the app to the Trash instead leaves the others behind.

### What has been run

What has been run on a Mac is listed in [Product](PRODUCT.md#what-is-not-established). `--self-test` records the main display, the output audio and the microphone for five seconds. The app was built, installed and run on one Apple Silicon Mac with macOS 26 on 2026-11-18 with synthetic meetings, and on 2026-11-19 with Slack huddles and Google Meet calls; [the first verification note](docs/note/note-20261118-macos-verification-2026-11-18.md) and [the notes of 2026-11-19](docs/note/note-20261119-macos-verification-2026-11-19.md) list what was and was not observed.
