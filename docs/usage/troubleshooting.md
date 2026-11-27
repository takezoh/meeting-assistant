# Troubleshooting

Start with the log. The tray menu opens the log folder (`%LOCALAPPDATA%\MeetingAssistant\logs`). Every detection start and end, every recording start, stop and failure, and every problem with a capture source or the extension link is written there.

## A meeting was not recorded

Run `meeting-assistant --probe` while the meeting is in progress.

| What `--probe` shows | Cause and remedy |
| --- | --- |
| The meeting app is not listed as using the microphone | The app has not opened the microphone, for example because you joined listen-only. Record it by hand from the status panel. Browser meetings are detected only by the browser extension |
| The process is listed but no rule is satisfied | No rule names that process. Add a rule; see [Configuration](configuration.md#tuning-with---probe) |
| The rule is satisfied | Check that automatic recording is not paused in the tray menu, then read the log for a start failure |

## The recording stopped during the meeting

The meeting app stopped using the microphone for longer than `end_grace_seconds`. An app that releases the microphone while you are muted ends the meeting during a longer mute. Increase `end_grace_seconds`, or start the recording manually; a manual recording is not ended by detection.

## The recording does not stop after the meeting

The meeting app, or another process of it, still uses the microphone. `meeting-assistant --probe` lists the processes using the microphone. The recording ends `end_grace_seconds` after the app lets it go.

## The picture is black or frozen

- **Black from the start, or throughout:** the meeting's window has not been found. The picture appears when it is. A Slack huddle is black while it is previewed and shows from the moment it is joined. For other apps, a window that was already open when the app took the microphone is not taken for the meeting's, so a call window that opens just before the microphone is taken stays unrecorded. The app's main window and the whole display are deliberately not recorded instead.
- **Frozen:** the recorded window is minimized, or it was closed while the meeting went on. Audio continues.
- **Black bars:** the window's shape differs from the picture's (`video.width` by `video.height`, 16:9 by default), and the window is scaled to fit with its aspect ratio kept. A browser tab's picture takes the tab's shape when the recording starts; it shows bars after the tab changed size during the recording, because the picture keeps its size.

## The wrong window was recorded

For apps with several windows the largest one is chosen. A shared screen shown in a separate window is not recorded. Use `capture_scope = "screen"` if you need whatever is on the display.

## There is no sound from the other participants

When the other side's sound cannot be captured, a warning notification says so as the recording starts, with the reason.

- On macOS, Meeting Assistant is not allowed under System Settings > Privacy & Security > Screen & System Audio Recording. The meeting app's sound is captured through it. Allow it and start the app again.
- In app scope only the meeting app's audio is recorded. If the app plays audio through a helper process that is not its descendant, it is not captured. The log names an audio source that could not be opened.
- `audio.record_system_audio` may be `false`.

## My own voice is missing

Your voice is the recording's second audio track. Many players play only the first; check with a tool that lists the tracks, such as `ffprobe`. When the microphone cannot be opened, a warning notification says so as the recording starts.

- The microphone is muted in Windows, not just in the meeting. The log has a warning at the start of the recording.
- On macOS, Meeting Assistant is not allowed under System Settings > Privacy & Security > Microphone. macOS still opens the microphone but gives only silence; the app checks the permission and warns as the recording starts.
- `audio.record_microphone` is `false`.
- For a tab recording, a file named `<recording>.microphone.m4a` beside the recording means the microphone could not be added to it; the log says why. It holds your voice from slightly before the recording started.

## A browser meeting is recorded as a window, not a tab

The extension is not connected. Check, in this order:

1. The extension is loaded and enabled on the browser's extensions page.
2. The app is running and was installed, which registers it with the browser. Running an executable that was not installed does not.
3. The log shows the extension connecting when the browser starts.

## The tab recording does not start

The log shows the extension's error. The usual cause is that the browser was not started from the `(Meeting Assistant)` shortcut, so the switch that permits capture without a click is missing. Quit the browser completely, including background processes, and start it from the shortcut. If the switch is present and the error remains, the browser version may no longer support it.

If the error says the app and the extension are of different versions, the browser runs an extension other than the one the running app installed. Run `--install` from the app you use, then press **Reload** for the extension on the browser's extensions page.

## The app is not running after logon

Check **Start at logon** in the tray menu, or run `--install` again. An invalid `config.toml` also stops the app at startup; the message names the problem. A file from an older version does not: it is set aside and replaced with the defaults.

## My settings are gone after an update

The new version stores its settings in another format. Your old file is beside `config.toml` as `config.toml.unversioned.bak` or `config.toml.v<version>.bak`; copy the values you need into the new file and start the app again.

## Notifications are attributed to Windows PowerShell

This is a known defect: the app does not register itself as a notification publisher. The notifications themselves are from Meeting Assistant.

## A file is left after a crash

The file is playable up to shortly before the crash. Nothing needs to be repaired. Until the app starts again it is in the `in-progress` folder inside the recordings folder; at the next start the app moves it into the recordings folder and says so.
