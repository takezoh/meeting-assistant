---
id: task-20260904-echo-leak-measurement
kind: task
title: echo-leak-measurement
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-capture/src/wasapi/leak_measure.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-process-loopback-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Compute the echo return loss between the paired loopback and microphone tracks
  by the method adr-phase1-echo-leak-measurement-representation fixes, and emit it
  as a per-application measurement record.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-echo-leak-measurement.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Compute the echo return loss between the paired loopback and microphone tracks by the method adr-phase1-echo-leak-measurement-representation fixes, and emit it as a per-application measurement record.

## Execution contract

- Output: One Rust module under crates/ma-capture/src/wasapi computing frame RMS, window selection and the difference of levels, plus a serialisable per-application record and fixture tests.
- Tool guidance: Do not reuse Payload.level_dbfs and do not add a Payload field; the measurement is capture-side data derived from two tracks, not an observation of one subject at one instant.
- Boundaries: Does not open any audio stream, does not decide the capture mode, and does not write the Windows-tier record file (the manual procedure does).

## Acceptance

- Given paired loopback and microphone fixture tracks with a synthesised 18 dB echo return loss, the computed value is 18 dB plus or minus 1 dB, and the record carries the window's start sample on each track, both RMS dBFS values and the alignment uncertainty in milliseconds.
- Given a fixture in which no sixty-second window satisfies both the loopback-active and no-local-speech conditions, the outcome recorded is no_qualifying_window and no number is produced; given a session whose alignment uncertainty exceeds one second, the outcome recorded is inconclusive_alignment.
- Given the measurement, no Signal is emitted and no Payload field is written; the value is carried in the per-application measurement record only.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit f439c3d38892 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit f439c3d38892 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
