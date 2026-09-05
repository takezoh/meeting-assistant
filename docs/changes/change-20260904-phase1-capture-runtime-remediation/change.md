---
id: change-20260904-phase1-capture-runtime-remediation
kind: change
title: Phase 1 capture runtime remediation
status: active
created: '2026-09-04'
profile: sdd@1
intent: 'Close the capture-runtime invariants left open by Phase 1 validation: WASAPI
  servicing must be isolated from durable I/O, microphone identity must survive endpoint
  loss, and persistence failure must stop further evidence reads.'
outcomes:
- WASAPI packet servicing is decoupled from chunk writes and fsync through a bounded
  handoff whose overflow is represented as an explicit gap.
- A lost microphone endpoint can wait or reopen capture-device mode but can never
  turn the microphone track into render loopback.
- Any signal persistence failure terminates the live session before another source
  read and is returned as a failure.
scope:
- crates/ma-capture/src/wasapi/mod.rs
- crates/ma-capture/src/wasapi/mic_endpoint.rs
- crates/ma-engine/src/bin/ma-diag.rs
- crates/ma-engine/tests/diagnostic_cli.rs
non_goals:
- Endpoint notification lifecycle, detector candidate ownership, manual-record schemas,
  and Windows evidence collection are separate follow-up changes.
change_classes:
- behavior
- responsibility
- invariant
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260904-phase1-capture-runtime-remediation/requirements.md
  required: true
- role: implementation
  path: changes/change-20260904-phase1-capture-runtime-remediation/implementation.md
  required: true
- role: verification
  path: changes/change-20260904-phase1-capture-runtime-remediation/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- phase-1
- capture
- remediation
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Isolate WASAPI reads from durable writes and preserve microphone and signal
  identity across runtime failures.
updated: '2026-09-04'
---

## Summary

Phase 1 validation showed that moving capture into a worker thread did not isolate
the WASAPI pump from chunk durability I/O, and that failure paths could change a
microphone track into render loopback or continue after losing persisted signals.
This follow-up change owns only those three runtime invariants.

## Closure Notes

Pending implementation and verification.
