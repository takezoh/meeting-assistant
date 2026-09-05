---
id: change-20260905-phase1-audio-endpoint-lifecycle-remediation
kind: change
title: Phase 1 audio endpoint lifecycle remediation
status: active
created: '2026-09-05'
profile: sdd@1
intent: Preserve notification-source transitions and recreate stale endpoint registrations,
  including same-id reconnects between polls.
outcomes:
- Queued session callbacks survive endpoint teardown and retain callback order ahead
  of reconciliation.
- An active endpoint whose existing manager is stale is removed and freshly registered
  even when its device id is unchanged.
scope:
- crates/ma-signals-windows/src/audio_session.rs
non_goals:
- Capture endpoint selection persistence and collection of live Windows evidence are
  separate changes.
change_classes:
- behavior
- invariant
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260905-phase1-audio-endpoint-lifecycle-remediation/requirements.md
  required: true
- role: implementation
  path: changes/change-20260905-phase1-audio-endpoint-lifecycle-remediation/implementation.md
  required: true
- role: verification
  path: changes/change-20260905-phase1-audio-endpoint-lifecycle-remediation/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- phase-1
- audio-session
- remediation
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Preserve queued session callbacks and refresh stale same-id endpoint registrations.
updated: '2026-09-05'
---

## Summary

The live endpoint refresh path currently drops callback evidence during teardown
and treats device-id equality as proof that an existing session manager remains
valid. Both assumptions fail for short sessions and same-id device reconnects.

## Closure Notes

Implemented and mechanically verified. The change remains active with the Phase 1
integration until the required live Windows observations are collected.
