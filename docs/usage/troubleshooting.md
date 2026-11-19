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

The meeting app stopped using the microphone for longer than `end_grace_seconds` while no window that opened with the meeting was open. Some apps release the microphone while you are muted; if such an app shows the call in its main window, the meeting ends. Increase `end_grace_seconds`, or start the recording manually; a manual recording is not ended by detection.

## The recording does not stop after the meeting

A window that opened with the meeting is still open, for example a call window that stays after the call. The recording ends when it is closed.

## The picture is black or frozen

- **Black from the start, or throughout:** no window appeared with the meeting. The picture appears when the meeting's window does. A window that was already open when the app took the microphone is not taken for the meeting's, so a call window that opens just before the microphone is taken stays unrecorded. The app's main window and the whole display are deliberately not recorded instead.
- **Frozen:** the recorded window is minimized, or it was closed while the meeting went on. Audio continues.
- **Black bars:** the window's shape differs from the picture's (`video.width` by `video.height`, 16:9 by default), and the window is scaled to fit with its aspect ratio kept.

## The wrong window was recorded

For apps with several windows the largest one is chosen. A shared screen shown in a separate window is not recorded. Use `capture_scope = "screen"` if you need whatever is on the display.

## There is no sound from the other participants

- In app scope only the meeting app's audio is recorded. If the app plays audio through a helper process that is not its descendant, it is not captured. The log names an audio source that could not be opened.
- `audio.record_system_audio` may be `false`.

## My own voice is missing

Your voice is the recording's second audio track. Many players play only the first; check with a tool that lists the tracks, such as `ffprobe`.

- The microphone is muted in Windows, not just in the meeting. The log has a warning at the start of the recording.
- On macOS, Meeting Assistant is not allowed under System Settings > Privacy & Security > Microphone. The track is then silent.
- `audio.record_microphone` is `false`.
- For a tab recording, a file named `<recording>.microphone.m4a` beside the recording means the microphone could not be added to it; the log says why. It holds your voice from slightly before the recording started.

## A browser meeting is recorded as a window, not a tab

The extension is not connected. Check, in this order:

1. The extension is loaded and enabled on the browser's extensions page.
2. The app is running and was installed with `--install`, which registers it with the browser.
3. The log shows the extension connecting when the browser starts.

## The tab recording does not start

The log shows the extension's error. The usual cause is that the browser was not started from the `(Meeting Assistant)` shortcut, so the switch that permits capture without a click is missing. Quit the browser completely, including background processes, and start it from the shortcut. If the switch is present and the error remains, the browser version may no longer support it.

## The app is not running after logon

Check **Start at logon** in the tray menu, or run `--install` again. An invalid `config.toml` also stops the app at startup; the message names the problem. A file from an older version does not: it is set aside and replaced with the defaults.

## My settings are gone after an update

The new version stores its settings in another format. Your old file is beside `config.toml` as `config.toml.unversioned.bak` or `config.toml.v<version>.bak`; copy the values you need into the new file and start the app again.

## Notifications are attributed to Windows PowerShell

This is a known defect: the app does not register itself as a notification publisher. The notifications themselves are from Meeting Assistant.

## A file is left after a crash

The file is playable up to shortly before the crash. Nothing needs to be repaired.
