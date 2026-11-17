---
id: issue-20261117-notifications-show-windows-powershell-as
kind: issue
title: Notifications show Windows PowerShell as the publisher
status: open
created: '2026-11-17'
issue_type: bug
tags: []
owners: []
relations: []
subject_paths:
- crates/app/src/platform/win/mod.rs
summary: Toast notifications from the app are attributed to Windows PowerShell because the app registers no notification identity.
---

## Symptom

Toast notifications raised by Meeting Assistant on Windows are shown with "Windows PowerShell" as the sender instead of the app's name and icon.

## Evidence

`crates/app/src/platform/win/mod.rs` shows notifications through `notify_rust` without an application user model ID. The installer in `crates/app/src/platform/win/install.rs` registers a Start Menu shortcut and an uninstall entry but no notification identity, so Windows attributes the toast to the library's default identity.

## Context for follow-up

Windows attributes a toast through an AppUserModelID that is set on a Start Menu shortcut. Registration belongs behind `InstallSystem` and is governed by the [installation design](../design/design-installation.md); uninstall must remove whatever is added. The notification text and when notices are raised are not part of this defect.
