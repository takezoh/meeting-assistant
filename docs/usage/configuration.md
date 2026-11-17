# Configuration

Meeting Assistant reads one TOML file, `%APPDATA%\MeetingAssistant\config.toml` on Windows and `~/Library/Application Support/MeetingAssistant/config.toml` on macOS. It is created with the defaults the first time the app starts. The tray menu opens it.

Changes take effect the next time the app starts. A file that cannot be parsed, contains an unknown key or fails validation stops the app at startup with a message that names the file and the reason; nothing is applied partially. Keys you leave out keep their defaults.

## Settings

| Key | Default | Meaning |
| --- | --- | --- |
| `output_directory` | `Videos\MeetingAssistant` (`~/Movies/MeetingAssistant` on macOS) | Folder for recordings |
| `start_debounce_seconds` | `3.0` | How long the start condition must hold before a meeting is considered started |
| `end_grace_seconds` | `15.0` | How long microphone use must be absent before the meeting is considered ended |
| `poll_interval_milliseconds` | `1000` | How often signals are read. At least 100 |
| `capture_scope` | `"app"` | `"app"` records the meeting app only. `"screen"` records the display that holds the meeting window and all audio of the output device |
| `video.framerate` | `30` | Frames per second, 1 to 60 |
| `video.quality` | `70` | Encoder quality, 1 to 100. Higher is sharper and larger |
| `video.fallback_bitrate_kbps` | `4000` | Average bitrate used when the encoder does not support a quality setting, 100 to 100000 |
| `audio.record_system_audio` | `true` | Record the other side: the app's audio in app scope, the output device in screen scope |
| `audio.record_microphone` | `true` | Record your microphone |
| `rules` | See below | What counts as a meeting. Must not be empty |

The video and audio settings apply to recordings made by the app itself. A tab recording made by the browser extension uses the browser's encoder with the extension's own bitrate.

## Rules

A rule says which processes can be in a meeting.

```toml
[[rules]]
name = "Zoom"
process_names = ["zoom"]

[[rules]]
name = "WebMeeting"
process_names = ["chrome", "msedge"]
require_window_title = true
window_title_patterns = ['^Meet\s*[-–—:]\s', '\bMicrosoft Teams\b']
```

| Key | Meaning |
| --- | --- |
| `name` | Used in the recording's file name and in the log |
| `process_names` | Executable names without extension, case-insensitive. Child processes of a matching process count as that process |
| `require_window_title` | When `true`, the start also requires a window of the process whose title matches a pattern. Use it for programs, such as browsers, that use the microphone for other things |
| `window_title_patterns` | Case-insensitive regular expressions tested against window titles |

A rule starts a meeting when one of its processes has used the microphone for `start_debounce_seconds`, and, if `require_window_title` is set, a title matches at that time. The title is needed only to start; the meeting then lasts as long as the microphone is in use.

The first rule in the file that is satisfied names the recording. Writing a `rules` list replaces the defaults entirely, so copy the default rules from the generated file before editing them.

## Tuning with `--probe`

```
meeting-assistant --probe
```

prints, for this moment, the processes with an active microphone session, the windows that were found and each rule's verdict. To add an app:

1. Join a call in the app and run `--probe`.
2. Take the process name listed as using the microphone and add a rule with it.
3. Run `--probe` again and confirm the rule is satisfied, then restart the app.

For a browser-based service, read the window title while in a call and add a pattern that matches it but not ordinary pages.

## Choosing the timings

- A longer `start_debounce_seconds` avoids recordings of a brief microphone test at the cost of missing the first seconds.
- A longer `end_grace_seconds` keeps one file across short interruptions, for example when an app releases the microphone while you are muted, at the cost of a longer tail after the meeting.

## Using another file

`meeting-assistant --config <path>` reads that file instead and writes logs to a `logs` folder beside it. This is how a development build runs next to an installed copy's settings.
