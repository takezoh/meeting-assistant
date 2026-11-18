---
id: issue-20261118-a-tab-recording-saved-as-webm-is-reporte
kind: issue
title: A tab recording saved as WebM is reported with an .mp4 name
status: done
created: '2026-11-18'
issue_type: bug
tags: []
owners: []
relations: []
subject_paths:
- crates/core/src/coordinator.rs
- crates/app/src/browser/server.rs
summary: Notices, the log and the status panel name a WebM tab recording with the
  .mp4 path chosen before it started.
disposition_reason: 'Fixed: the browser link renames the file when the container is
  reported, and Recorder::start returns the file it writes, which the coordinator
  reports.'
updated: '2026-11-18'
---

## Symptom

When the extension records a tab as WebM, the file on disk ends in `.webm`, but the "Recording started" and "Recording saved" notices, the log line "Recording saved" and the status panel's file name all say `.mp4`.

## Evidence

On a Mac on 2026-11-18, with a stand-in for the extension that reported `mime: video/webm;codecs=vp9,opus`, the app log showed:

```
WARN  Saved with a different extension to match the format the browser recorded: …/2026-11-18_18-12-56_Whereby.webm
INFO  Recording saved: …/2026-11-18_18-12-56_Whereby.mp4
```

and only `2026-11-18_18-12-56_Whereby.webm` existed in the recordings folder. The panel showed `2026-11-18_18-12-56_Whereby.mp4` while recording.

## Context

The coordinator names the file before the recording starts and keeps that path in its status and notices; the browser link renames the file to match the container the extension reports. The [browser link reference](../technical/browser-link.md) says the app names the file by the reported container.
