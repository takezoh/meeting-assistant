---
id: task-20260904-closed-schema-discipline
kind: task
title: closed-schema-fixture-guardrail
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-signal/tests/phase1_schema_guard.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-replayable-timeline-fixtures}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Prove that Phase 1's collectors, capture measurements and extension path
  added no field, variant or free-text value to the closed signal schema.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-closed-schema-discipline.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Prove that Phase 1's collectors, capture measurements and extension path added no field, variant or free-text value to the closed signal schema.

## Execution contract

- Output: One test file in ma-signal asserting fixture conformance and the frozen field and variant sets.
- Tool guidance: This unit adds no production code path; it only asserts the absence of drift.
- Boundaries: Does not implement any collector, capture source, harness or detector change.

## Acceptance

- Given every committed Phase 1 fixture line, each signal validates against contracts/signal/signal-envelope.schema.json and the existing schema_contains_no_free_text_subject and schema_golden_roundtrip tests still pass unchanged.
- Given the Payload and Subject types, the test asserts the exact field set (restart_resync, audible, level_dbfs, command, calendar_event_key, process_tree_root_pid) and the exact four Subject variants, so an added field fails here before it reaches a fixture.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit bc704a3ab873 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit bc704a3ab873 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
