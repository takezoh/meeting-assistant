---
id: task-20260904-mic-endpoint-follows-session
kind: task
title: mic-endpoint-follow-session
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-capture/src/wasapi/mic_endpoint.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-process-loopback-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Select the microphone endpoint the meeting application is using from an endpoint
  identifier the composition root supplies, and re-evaluate on endpoint change through
  the existing successor-track mechanism.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-mic-endpoint-follows-session.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Select the microphone endpoint the meeting application is using from an endpoint identifier the composition root supplies, and re-evaluate on endpoint change through the existing successor-track mechanism.

## Execution contract

- Output: One Rust module under crates/ma-capture/src/wasapi implementing endpoint selection over an explicit Option<&str> endpoint argument, plus fixture tests.
- Tool guidance: Reuse TrackSegment::open_successor and SourceEvent::FormatChanged; do not invent a parallel device-change path and do not add any dependency, type import or trait from ma-signals-windows.
- Boundaries: Does not observe the session endpoint (windows-audio-session-mic-collector owns that) and does not wire the two together (diagnostic-harness-composition-root owns that).

## Acceptance

- Given an endpoint identifier supplied as an explicit function argument by the caller, the microphone CaptureSource opens that endpoint rather than the system default communications device; given None, it opens the system default and records that it did.
- Given a supplied endpoint change delivered mid-recording, the existing TrackSegment::open_successor and SourceEvent::FormatChanged path is used and the successor track's origin names the new endpoint.
- Given cargo xtask boundary, ma-capture declares no dependency on ma-signals-windows; the endpoint identifier crosses as a string argument from the composition root, not as a crate edge.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit e65d146afc43 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit e65d146afc43 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
