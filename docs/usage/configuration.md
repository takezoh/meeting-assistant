# Configuration

Meeting Assistant reads one TOML file, `%APPDATA%\MeetingAssistant\config.toml` on Windows and `~/Library/Application Support/MeetingAssistant/config.toml` on macOS. It is created with the defaults the first time the app starts. The tray menu opens it.

Changes take effect the next time the app starts. Keys you leave out keep their defaults.

The file starts with `version`, the format of the settings; leave it as it is. A file of another version, including one written by an earlier release that did not number its settings, is not read: the app moves it to `config.toml.unversioned.bak` (or `config.toml.v<version>.bak`) beside the new one, writes the defaults and starts with them, and says so in a notification and the log. Settings from the old file are not carried over; copy back what you need.

A file of the current version that cannot be parsed, contains an unknown key or fails validation stops the app at startup with a message that names the file and the reason; nothing is applied partially, and the file is left for you to fix.

## Settings

| Key | Default | Meaning |
| --- | --- | --- |
| `version` | `1` | The format of this file. Not a setting; a file of another version is replaced with the defaults |
| `output_directory` | `Videos\MeetingAssistant` (`~/Movies/MeetingAssistant` on macOS) | Folder for recordings. A recording in progress is written in its `in-progress` subfolder and moved into it when finished |
| `start_debounce_seconds` | `3.0` | How long a desktop app must use the microphone before its meeting is considered started |
| `end_grace_seconds` | `2.0` | How long a desktop app must not have used the microphone before its meeting is considered ended, whatever windows it still has |
| `browser_start_debounce_seconds` | `1.0` | How long the extension must report a call before the browser meeting is considered started |
| `browser_end_grace_seconds` | `1.0` | How long the extension must report no call before the browser meeting is considered ended |
| `poll_interval_milliseconds` | `1000` | How often signals are read. At least 100 |
| `capture_scope` | `"app"` | `"app"` records the meeting app only. `"screen"` records the display that holds the meeting's window (the main display when it has none) and all audio of the output device (on macOS, everything apps play, whichever device they play to) |
| `video.width`, `video.height` | `1920`, `1080` | Size of the picture, even numbers from 320x240 up to 1920x1080. The recorded window or display is fitted into it with its aspect ratio kept, also when the window changes size or shape: a larger one is scaled down; on Windows a smaller one keeps its size in the middle |
| `video.framerate` | `30` | Frames per second, 1 to 60 |
| `video.quality` | `70` | Encoder quality, 1 to 100. Higher is sharper and larger |
| `video.fallback_bitrate_kbps` | `4000` | Average bitrate used when the encoder does not support a quality setting, 100 to 100000 |
| `audio.record_system_audio` | `true` | Record the other side, the first audio track: the app's audio in app scope, the output device in screen scope. When `false` that track is silent |
| `audio.record_microphone` | `true` | Record your microphone, the second audio track, also for browser tabs. When `false` that track is silent |
| `rules` | See below | What counts as a meeting. Must not be empty |

The video and audio settings apply to recordings made by the app itself. A tab recording made by the browser extension uses the browser's encoder with the extension's own bitrate.

## Rules

A rule names the processes of a desktop meeting app. Browser meetings are not configured here; they are detected by the browser extension.

```toml
[[rules]]
name = "Zoom"
process_names = ["zoom"]
```

| Key | Meaning |
| --- | --- |
| `name` | Used in the recording's file name and in the log |
| `process_names` | Executable names without extension, case-insensitive. Child processes of a matching process count as that process |

A rule starts a meeting when one of its processes has used the microphone for `start_debounce_seconds`. The meeting's window is recorded: for Slack the huddle window, which the app tells from Slack's main window; for other apps a window that appears once the microphone is taken, such as a call window. Until it is found the picture is black. The app's other windows, its main window among them, are not recorded. Which apps the app knows more about is built in, not configured. The meeting lasts while the microphone is in use, whatever windows are open.

The first rule in the file that is satisfied names the recording. Writing a `rules` list replaces the defaults entirely, so copy the default rules from the generated file before editing them.

## Tuning with `--probe`

```
meeting-assistant --probe
```

prints, for this moment, the processes with an active microphone session, the windows that were found and each rule's verdict. To add an app:

1. Join a call in the app and run `--probe`.
2. Take the process name listed as using the microphone and add a rule with it.
3. Run `--probe` again and confirm the rule is satisfied, then restart the app.


## Choosing the timings

- A longer `start_debounce_seconds` avoids recordings of a brief microphone test at the cost of missing the first seconds.
- A longer `end_grace_seconds` keeps one file across short interruptions, for example when an app releases the microphone while you are muted, at the cost of a longer tail after the meeting.
- Browser meetings have their own timings. A connected call does not come and go while you are muted, and the extension counts a call that is reconnecting as connected, so the recording can stop a second after you leave.

## Using another file

`meeting-assistant --config <path>` reads that file instead and writes logs to a `logs` folder beside it. This is how a development build runs next to an installed copy's settings.
