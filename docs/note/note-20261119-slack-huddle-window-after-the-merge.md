---
id: note-20261119-slack-huddle-window-after-the-merge
kind: note
title: Slack huddle window after the merge, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/detector.rs
- crates/core/src/rules.rs
summary: 'Two huddles on one Mac: windows read only from the microphone left the huddle
  black; with Slack''s main window mark the huddle window was recorded from joining.'
---

## Summary

On 2026-11-19, on one Mac (macOS 26) with Slack (desktop, English), the user previewed, joined and left a huddle twice, once with each of two builds installed. With the merged build of d0a8ed3, which reads the windows only from the moment some process uses the microphone, the picture was black throughout. With a build that also takes Slack's mark of its main window into account, the huddle window was recorded from the moment of joining and the main window was not. One huddle each, alone in the huddle; Windows and other apps were not tried. The mark was then moved from a rule setting into a Slack adapter ([ADR](../adr/adr-20261119-tell-meeting-windows-apart-through-app-adapters.md)), and that build recorded a third huddle the same way. Its recording ended 18.7 s after leaving; a fourth huddle, watched every 200 ms, showed that Slack hides the huddle window when the huddle is left and destroys it about 13 s later, and the macOS window list, which then included windows off screen, kept the meeting going meanwhile. The list was changed to windows on screen only. In a fifth huddle with that build the recording ended 2.8 s after Slack let go of the microphone, but Slack destroyed its hidden window 0.45 s after hiding it that time, so the change was not what ended it; that a hidden window leaves the on-screen list was checked with a window of a test program instead. The meeting was then made to end on the microphone alone ([ADR](../adr/adr-20261119-end-a-desktop-meeting-on-the-microphone.md)); in a sixth huddle Slack kept its hidden window for 27.6 s and the recording still ended 2.7 s after Slack let go of the microphone, and it went on through a mute.

## Notes

### Build d0a8ed3: windows read from the microphone on

| Time | Event |
| --- | --- |
| 14:29:29.7 | the app saw the microphone in use and read the windows for the first time: two windows of meeting apps open |
| 14:29:33.1 | recording started with no window (black) |
| about 14:29:42 | Slack's join sound, 9 s into the recording |
| about 14:30:11 | Slack's leave sound, 38 s into the recording |
| 14:30:13.8 | the recording ended, 40.7 s long |

All 1221 frames were black (largest luma 16). Both audio tracks had sound. Read with `--probe` after the huddle, Slack had one window, titled "dev-gcp-errors (Channel) - tkz - Slack", so the second window at the first reading was the huddle's. Slack had taken the microphone when the huddle was previewed, about 12 s before joining, with the huddle window already open.

### Build with the main window mark

In this build the mark was a rule setting, `main_window_title = "[Main]"`, added to the Slack rule in the user's `config.toml`; the decision logic was the same as the Slack adapter's that replaced it.

| Time | Event |
| --- | --- |
| 15:08:59.1 | first reading of the windows: two open, none marked as Slack's main window |
| 15:09:03.4 | recording started with no window (black) |
| 15:09:15.1 | the recording was asked to move to a window, and switched 73 ms later; Slack's join sound is 11 s into the recording |
| about 15:09:29.5 | Slack's leave sound, 26 s into the recording |
| 15:09:34.2 | the recording ended, 30.8 s long |

The first 351 frames (11.7 s) were black; from then on the picture was the huddle window, headed "Huddle in dev-gcp-errors", including a mute; after leaving it faded to black for the last 4 s. Slack's main window did not appear in the recording. So Slack marks its main window when the huddle is joined, not while it is previewed.

From leaving to the end of the recording took about 4.4 s, against about 2.7 s with the first build, where no window kept the meeting going. The picture turned black about 0.3 s after the leave sound; when the huddle window left the window list was not logged.

### Also observed

- The log line "Switched the window being recorded." was written twice for one switch, once by the coordinator on asking and once by the pipeline on switching; the coordinator's line now says that it asked, and names the window.
- After each `--install` macOS asked again for the microphone, and the self-test waited for the answer until its 20-second limit, as recorded in [the permission issue](../issue/issue-20261119-every-update-drops-the-macos-screen-reco.md).

### Build with the Slack adapter

| Time | Event |
| --- | --- |
| 16:20:13.9 | first reading of the windows: two open, nothing told by the adapter |
| 16:20:17.2 | recording started with no window (black) |
| 16:20:33.9 | asked to move to window 0x632, switched 0.1 s later; Slack's join sound is 15 s into the recording |
| about 16:20:52 | Slack's leave sound, 35 s into the recording; the picture turned black |
| 16:21:10.9 | the recording ended, 53.8 s long |

The picture was the huddle window from 16.7 s to 35.1 s and black otherwise; Slack's main window did not appear. Leaving to the end of the recording took about 18.7 s.

### What Slack does when a huddle is left

With the same build installed, a watcher listed Slack's windows of at least 100 by 100 pixels, on screen or not, and the processes taking microphone input, every 200 ms, while the user previewed, joined and left a fourth huddle.

| Time | Slack's windows | Microphone |
| --- | --- | --- |
| 16:34:53.145 | main 0x3f; huddle 0x649 appears, 520x752 | — |
| 16:34:53.353 | the same | Slack Helper takes it |
| 16:34:55.3 to 16:34:56.0 | huddle turns 1224x714 on joining | Slack Helper lets go and takes it again |
| 16:36:35.8 | huddle 1200x700 | Slack Helper lets go (left) |
| 16:36:36.0 | huddle 0x649 off screen, still listed | — |
| 16:36:49.1 | huddle 0x649 gone | — |

The app ended the recording at 16:36:52.9, 17.1 s after leaving and 3.8 s after the hidden window went. This time the meeting started after joining, so the recording started on the huddle window. The huddle window opened 0.2 s before Slack took the microphone.

### Windows on screen only

The build that lists only windows on screen was installed and a fifth huddle was watched the same way.

| Time | Event |
| --- | --- |
| 16:59:17.1 | huddle window 0x68d appears; Slack takes the microphone 0.2 s later |
| 16:59:20.7 | recording started with no window (black) |
| 16:59:27.0 | joined: the huddle window turns 1224x714 |
| 16:59:27.7 | asked to move to window 0x68d, switched 0.1 s later |
| 16:59:43.85 | left: Slack lets go of the microphone |
| 16:59:44.08 | huddle window off screen |
| 16:59:44.53 | huddle window gone |
| 16:59:46.69 | the recording ended, 2.8 s after Slack let go of the microphone |

The picture was the huddle window from 6.7 s to 22.9 s of the 25.8-s recording and black otherwise; Slack's main window did not appear. How long Slack keeps a hidden huddle window varied: 13 s in the fourth huddle, 0.45 s here.

A test program opened a window and hid it with `orderOut`, as an app hides a window it keeps: the hidden window stayed in the list of all windows and left the list of windows on screen. So with this build a hidden huddle window no longer keeps a meeting going; a huddle in which Slack keeps it hidden for long has not been recorded with this build.

### Ending on the microphone alone

The build in which a desktop meeting continues and ends on its app's microphone use alone was installed, and a sixth huddle was watched the same way, with a mute.

| Time | Event |
| --- | --- |
| 18:35:30.7 | huddle window 0x6e7 appears; Slack takes the microphone 0.2 s later |
| 18:35:35.4 | recording started with no window (black) |
| 18:35:43.1 | joined: the huddle window turns 1224x714 |
| 18:35:44.4 | asked to move to window 0x6e7, switched 0.1 s later |
| 18:35:43.5 to 18:35:58.7 | Slack Helper takes microphone input without a break, through a mute |
| 18:35:58.69 | left: Slack lets go of the microphone |
| 18:35:58.91 | huddle window off screen |
| 18:36:01.37 | the recording ended, 2.7 s after Slack let go of the microphone |
| 18:36:26.5 | huddle window gone, 27.6 s after it went off screen |

The picture was the huddle window from 8.7 s to 23.0 s of the 25.8-s recording and black otherwise; Slack's main window did not appear. At 16 s the huddle's microphone button showed muted ("Unmute mic"), at 20 s unmuted, and the recording went on throughout.
