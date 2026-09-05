---
id: change-20260905-phase1-microphone-endpoint-evidence-export
kind: change
title: Phase 1 microphone endpoint evidence export
status: active
created: '2026-09-05'
profile: sdd@1
intent: Make the endpoint selection history required by the live microphone procedure
  observable outside the capture worker.
outcomes:
- The capture worker publishes initial and successor endpoint choices plus switch
  diagnostics.
- ma-diag writes a durable endpoint-selection sidecar adjacent to the session timeline.
scope:
- crates/ma-engine/src/bin/ma-diag.rs
non_goals:
- Endpoint matching policy, capture source behavior, and Windows evidence collection
  are unchanged.
change_classes:
- behavior
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260905-phase1-microphone-endpoint-evidence-export/requirements.md
  required: true
- role: implementation
  path: changes/change-20260905-phase1-microphone-endpoint-evidence-export/implementation.md
  required: true
- role: verification
  path: changes/change-20260905-phase1-microphone-endpoint-evidence-export/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- phase-1
- microphone
- evidence
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Export the live microphone endpoint selection history from the capture worker
  as a durable session sidecar.
updated: '2026-09-05'
---

## Summary

`MicEndpointSource` tracks the selected endpoint and successor history, but the
production worker owns and drops that value. The declared live procedure therefore
cannot obtain its required selection history from `ma-diag`.

## Closure Notes

Implemented and mechanically verified. Live collection of the Windows observation
remains part of the Phase 1 integration gate.
