---
id: task-20260904-meet-process-tree-corroboration
kind: task
title: meet-process-tree-corroboration
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-detect/src/detector.rs
- fixtures/signal-timelines/browser-tab-with-mic.jsonl
- fixtures/signal-timelines/browser-tab-cross-tree.jsonl
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-replayable-timeline-fixtures}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Implement the process-tree join FR-111 and the accepted extension-channel
  ADR require, and assert decision explainability against the real Windows-recorded
  fixtures.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-meet-process-tree-corroboration.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Implement the process-tree join FR-111 and the accepted extension-channel ADR require, and assert decision explainability against the real Windows-recorded fixtures.

## Execution contract

- Output: A change to decide()'s candidate keying and rule selection in crates/ma-detect/src/detector.rs, one updated and one new fixture, and tests in the same crate.
- Tool guidance: Keep decide() pure and its signature unchanged, keep the Outcome enum and partition() unchanged, and add the join inside candidate evaluation; the two new rule ids are process-tree-mismatch and process-tree-root-absent.
- Boundaries: Does not change adapter tables' corroboration flags, the evidence weights, the Outcome enum, or the generic-candidate fallback.

## Acceptance

- Given a browser-class adapter that requires both tab and microphone corroboration, decide() records each candidate's tab and microphone process_tree_root_pid and treats corroboration as met only when both are present and equal; browser-tab-with-mic.jsonl is updated so its mic_capture_started signal carries process_tree_root_pid 6300, matching the tab signal already committed with that value.
- Given the new browser-tab-cross-tree.jsonl fixture whose tab signal carries process_tree_root_pid 6300 and whose mic signal carries 7100, decide() emits Inconclusive with rule_id process-tree-mismatch and never a determinate start; given a tab signal with no process_tree_root_pid, decide() emits Inconclusive with rule_id process-tree-root-absent.
- Given the existing desktop fixtures, replay_is_byte_identical and the committed desktop-start-end.decisions.json remain unchanged, because a desktop-class adapter requires no tab evidence and the join rule does not apply to it.
- Given each Windows-recorded Phase 1 fixture, every emitted decision cites the exact signal ids and the adapter rule id, asserted against the committed decisions sidecar rather than by re-deriving it.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 5450b1cf1d70 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 5450b1cf1d70 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
