---
id: note-20261125-installing-from-the-windows-executable-2
kind: note
title: Installing from the Windows executable, 2026-11-25
status: draft
created: '2026-11-25'
tags: []
owners: []
relations:
- {type: references, target: adr-20261124-distribute-a-built-app-that-installs-itself}
- {type: references, target: design-installation}
- {type: references, target: issue-20261124-the-windows-installer-is-not-built}
source_paths:
- crates/app/build.rs
- crates/app/windows/resources.rs
- crates/app/windows/app.manifest
- crates/app/src/platform/win/install.rs
- packaging/windows/build.ps1
summary: On one Windows PC the built executable asked whether to install, changed
  nothing when declined, and, opened from a copy marked as downloaded, updated the
  installed app while it recorded. The build script made the unsigned executable;
  SmartScreen was not run.
---

## Summary

On 2026-11-25 the Windows installer of [the decision](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md) ran on one Windows 11 PC (10.0.26200). The build was commit 7401321 with the uncommitted changes that add it, built with `cargo build --release -p meeting-assistant`.

- **Resources:** Windows read the version information, the icon and the manifest from the executable.
- **Cancel:** opened without a command from outside the install location, the executable asked whether to install. Cancel changed nothing.
- **Update during a recording:** a copy marked as downloaded was opened while the installed app was recording, and OK was chosen. The running app finished its recording and exited, the new build was placed without the mark, and it started.

The dialogs were answered by posting the button's command to them, not by a click. `packaging/windows/build.ps1` then made the unsigned executable. SmartScreen was not run.

## Notes

### The executable's resources

Read from `target\release\meeting-assistant.exe` with `GetFileVersionInfoW`, `ExtractIconExW` and the resource enumeration functions:

- **Version information:** file and product version `0.1.0.0`, FileDescription and ProductName `Meeting Assistant`, FileVersion and ProductVersion `0.1.0`, InternalName `meeting-assistant`, OriginalFilename `meeting-assistant.exe`, in English with UTF-16 text.
- **Icon:** one icon group of six 32-bit images, 16 to 256 pixels. `LookupIconIdFromDirectoryEx` chose the image of the same size for 16, 32, 48 and 256 pixels.
- **Manifest:** resource ID 1. While the question was shown, the process was per-monitor v2 aware and had loaded comctl32 6.0.26100.9818 from WinSxS, so the manifest was in effect. The message box itself was system aware, at 120 dpi, the display's 125 %.

The test executable carried the same resources, which the new test reads.

### Cancel

The executable was opened twice from `target\release`, at 13:35:22 and 13:35:55, with the installed app (a build of 9c5ce64, PID 19652) running. Each time a message box titled Meeting Assistant asked:

> Install Meeting Assistant 0.1.0 for your user account? It will start when you log in, and it will be linked with Chrome and Edge so that it can record meetings in the browser. A Meeting Assistant that is already installed is replaced; if it is recording, that recording is finished first.

Its buttons were OK and Cancel. After Cancel the log said `The installation was declined.`, the process exited, and the installed executable had the same SHA-256 and time as before. The installed app went on running.

### An update while recording

1. The installed app was quit and started again with the panel's DevTools port, to drive the panel. At 13:36:55 it started an entire-screen recording with the microphone, written in `in-progress`.
2. A copy of the build was named `Meeting-Assistant-0.1.0.exe` and given a `Zone.Identifier` stream with `ZoneId=3`, as a browser marks a download. It was started directly, not through Explorer, so SmartScreen did not check it.
3. The same question appeared, and OK was chosen.

From the log:

| Time | What happened |
| --- | --- |
| 13:37:17.676 | The recording app received the quit request |
| 13:37:17.810 | `Recording saved`: 679 frames, 22.6 s, moved out of `in-progress`, which was then removed. The app exited |
| 13:37:18.186 | The installer's report: the app, the extension, the app list and start at logon, the native hosts and launchers for Chrome and Edge, and the start |
| 13:37:18.196 | The installed app started, now the new build |

The report was shown in a message box with the same lines. Afterwards:

- The installed `meeting-assistant.exe` had the SHA-256 of the build, and no `Zone.Identifier` stream. The copy that was opened still had its own.
- The `Run` value and the uninstall entry named the installed executable, with `DisplayVersion` 0.1.0.
- The recording `2026-11-25_13-36-54_Manual.mp4` held H.264 video at 1920x1080 (679 samples) and two AAC tracks, each 22.635 s, and the app and start time as metadata. Both tracks were silent: nothing played, and the only microphone was a muted virtual one.

### The build script

At 14:11 the user ran `powershell -ExecutionPolicy Bypass -File packaging/windows/build.ps1`, in Windows PowerShell 5.1 and without a certificate. It built the release executable, wrote `target\windows\Meeting-Assistant-0.1.0.exe`, said that the file was not signed, and printed its path. The file was 3,597,824 bytes, the same bytes as `target\release\meeting-assistant.exe`, and its version information said 0.1.0, which the script also checks. The executable installed above was built by `cargo build` before the script was run. The two differ in bytes; the only source change between them was a test.

### Not run

- `build.ps1` with a certificate, and in PowerShell 7.
- SmartScreen on a copy opened from Explorer, and the **Run anyway** step that INSTALL.md describes.
- Whether Windows would have warned about the installed app if it had kept the download's mark. The installer removes the mark because Windows checks a marked file when the shell starts it; that was not observed here.
- The installed app started from the Start menu or at logon after this installation, and Explorer's display of the icon and the details.
- Uninstalling from Settings > Apps, and signing.
