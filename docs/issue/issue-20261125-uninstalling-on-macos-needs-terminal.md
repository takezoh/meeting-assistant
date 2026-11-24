---
id: issue-20261125-uninstalling-on-macos-needs-terminal
kind: issue
title: Uninstalling on macOS needs Terminal
status: open
created: '2026-11-25'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: adr-20261124-distribute-a-built-app-that-installs-itself}
subject_paths:
- crates/app/src/tray.rs
- crates/app/src/platform/mac/install.rs
summary: A Mac user installs from a disk image without a terminal, but can uninstall
  only with --uninstall in Terminal; moving the app to the Trash leaves its login
  item, native-host manifests and launchers.
---

## Opportunity

Since [the decision to distribute a built app](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md), a Mac user installs Meeting Assistant from a disk image without a terminal. Removing it should not need one either.

## Evidence

As of 2026-11-25 the only way to uninstall on macOS is `meeting-assistant --uninstall`, run from the app's bundle in Terminal. Windows lists the app under Settings > Apps, whose uninstall entry runs the same command. macOS has no such list.

If the user moves `~/Applications/Meeting Assistant.app` to the Trash instead, these stay behind:

- `~/Library/LaunchAgents/com.meeting-assistant.agent.plist`, which then names a missing executable at every login
- the native-host manifests in the browsers' `NativeMessagingHosts` folders
- the browser launchers in `~/Applications`
- the extension in `~/Library/Application Support/MeetingAssistant`

## Context for follow-up

The procedure is `install::uninstall`, which already keeps configuration, logs and recordings. One way to reach it is an "Uninstall Meeting Assistant…" item in the menu bar menu. It would ask first and then run `--uninstall` in a separate process, as the tray's install item runs `--install`. The [installation design](../design/design-installation.md) INV-004 still applies.
