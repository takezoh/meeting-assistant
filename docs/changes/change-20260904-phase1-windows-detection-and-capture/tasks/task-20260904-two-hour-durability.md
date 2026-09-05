---
id: task-20260904-two-hour-durability
kind: task
title: two-hour-durability-harness
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-capture/tests/two_hour_durability.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-process-loopback-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Prove the two-hour durability window on the portable tier with a synthetic
  source and record the real two-hour run against a target application as a Windows-tier
  manual observation.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-two-hour-durability.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Prove the two-hour durability window on the portable tier with a synthetic source and record the real two-hour run against a target application as a Windows-tier manual observation.

## Execution contract

- Output: One integration test under crates/ma-capture/tests plus the manual procedure entry the registry unit registers.
- Tool guidance: Reuse CHUNK_SAMPLES, SAMPLE_RATE and QUEUE_CAP_SAMPLES from ma-capture; do not redefine chunk cadence for Windows sources and do not make the portable test depend on wall-clock time.
- Boundaries: Does not implement the capture source and does not change the chunk writer.

## Acceptance

- Given a SyntheticSource driven for 115_200_000 samples at 16 kHz, the chunk manifest names exactly 240 chunks, the chunk directory holds exactly those files, no gap record exists, and the total sample count equals the produced count.
- Given the Windows-tier manual procedure for v-win1-two-hour-live, the record states the target application, the wall-clock duration, the final manifest-versus-directory comparison and any gap records.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit ddd615909c9a on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit ddd615909c9a on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
