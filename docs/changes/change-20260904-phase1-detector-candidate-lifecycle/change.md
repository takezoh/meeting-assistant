---
id: change-20260904-phase1-detector-candidate-lifecycle
kind: change
title: Phase 1 detector candidate lifecycle remediation
status: active
created: '2026-09-04'
profile: sdd@1
intent: Prevent stale or foreign browser-tree evidence from surviving teardown and
  later producing a false meeting start.
outcomes:
- Process and microphone end signals clear only matching microphone evidence; tab
  teardown clears only matching tab evidence.
- A mismatched tab(A)+mic(B), followed by teardown of either side and new evidence,
  cannot start from stale corroboration.
scope:
- crates/ma-detect/src/detector.rs
non_goals:
- Active-meeting teardown, endpoint lifecycle, capture runtime, and manual verification
  are unchanged.
change_classes:
- behavior
- invariant
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260904-phase1-detector-candidate-lifecycle/requirements.md
  required: true
- role: implementation
  path: changes/change-20260904-phase1-detector-candidate-lifecycle/implementation.md
  required: true
- role: verification
  path: changes/change-20260904-phase1-detector-candidate-lifecycle/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- phase-1
- detector
- remediation
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Track and clear browser tab and microphone evidence independently by process
  tree.
updated: '2026-09-04'
---

## Summary

Phase 1 validation found that a candidate owns tab and microphone evidence separately
but tears it down as one aggregate. Mixed-tree evidence can therefore survive the wrong
end event and later create a false corroborated start.

## Closure Notes

Pending implementation and verification.
