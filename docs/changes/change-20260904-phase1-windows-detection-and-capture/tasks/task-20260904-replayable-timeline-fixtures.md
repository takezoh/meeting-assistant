---
id: task-20260904-replayable-timeline-fixtures
kind: task
title: signal-timeline-fixture-corpus
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- fixtures/signal-timelines/teams-desktop-session.jsonl
- fixtures/signal-timelines/slack-huddle-session.jsonl
- fixtures/signal-timelines/zoom-desktop-session.jsonl
- fixtures/signal-timelines/meet-chrome-with-extension.jsonl
- fixtures/signal-timelines/meet-chrome-without-extension.jsonl
- fixtures/signal-timelines/*.labels.json
- fixtures/signal-timelines/*.decisions.json
- crates/ma-signal/tests/phase1_fixture_shape.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-diagnostic-session-harness}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Produce the five Windows-recorded, redacted signal-timeline fixtures with
  their label and decisions sidecars, and the shape tests that keep them in the existing
  format.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-replayable-timeline-fixtures.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Produce the five Windows-recorded, redacted signal-timeline fixtures with their label and decisions sidecars, and the shape tests that keep them in the existing format.

## Execution contract

- Output: JSONL timeline files with .labels.json and .decisions.json sidecars under fixtures/signal-timelines/, plus one shape and redaction test file in ma-signal.
- Tool guidance: Reuse TimelineHeader verbatim; do not add a header field, a fixture format, or a new SignalKind; record the synthetic-identifier mapping in the manual record rather than in the fixture.
- Boundaries: Does not change ma-detect's replay path, does not change the detector's join rule, and does not record the confirmation label itself (the harness command does).

## Acceptance

- Given a diagnostic session recorded against each of the five targets (four applications, Meet with and without the extension), the merged timeline is written in the existing TimelineHeader-plus-JSONL shape and replays byte-identically through ma-detect's existing replay path against its committed decisions sidecar.
- Given every committed Phase 1 fixture, machine_profile is exactly "redacted", every pid is a synthetic value from a documented mapping, and no image name, package family name or tab host is a real service identifier; the real observed identifiers are recorded only in the Windows-tier manual record and asserted by the L4 adapter crates' own fixtures.
- Given every committed Phase 1 fixture, a .labels.json sidecar exists carrying at least one was_meeting entry in the existing shape.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit fa5ebdba2498 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit fa5ebdba2498 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
