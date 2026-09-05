---
id: change-20260905-phase1-manual-record-semantic-validation
kind: change
title: Phase 1 manual record semantic validation
status: active
created: '2026-09-05'
profile: sdd@1
intent: Require evidence-bearing observation values, not merely arrays or maps with
  the expected cardinality.
outcomes:
- Process identity capture and replay arrays reject null, empty, or false placeholders.
- Loopback activation maps and microphone endpoint history reject missing or non-concrete
  values.
scope:
- xtask/src/manual_record.rs
non_goals:
- Collection of the nine Windows observations and changes to their procedure text
  remain outside this change.
change_classes:
- behavior
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260905-phase1-manual-record-semantic-validation/requirements.md
  required: true
- role: implementation
  path: changes/change-20260905-phase1-manual-record-semantic-validation/implementation.md
  required: true
- role: verification
  path: changes/change-20260905-phase1-manual-record-semantic-validation/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- phase-1
- manual-verification
- remediation
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Reject placeholder arrays, maps, and endpoint histories that satisfy only
  observation counts.
updated: '2026-09-05'
---

## Summary

Phase 1 review found that several manual-verification observations satisfy their
validator with cardinality alone. Arrays filled with `null` or `false`, maps whose
values are `null`, and repeated empty endpoint identifiers can therefore support a
passing record without recording the evidence named by the procedure.

## Closure Notes

Implemented in `xtask/src/manual_record.rs`. Placeholder-valued process identity,
loopback activation, and microphone endpoint observations are rejected; concrete
counterexamples and valid examples are covered by unit tests. The change remains
active with the Phase 1 integration until the Windows evidence is collected.
