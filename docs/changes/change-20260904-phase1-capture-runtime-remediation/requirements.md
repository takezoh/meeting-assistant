---
change: change-20260904-phase1-capture-runtime-remediation
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Runtime requirements

- CRR-001: While a Windows capture source is active, filesystem writes and fsync
  must not prevent the source from servicing its 200 ms WASAPI buffer.
- CRR-002: When the bounded handoff cannot accept captured samples, the recording
  must contain an explicit gap; it must not silently discard samples or block the
  capture callback indefinitely.
- CRR-003: When a microphone endpoint is invalidated, the microphone track must
  never activate system render loopback. It may wait for a new endpoint or reopen
  a capture device with an explicit origin transition.
- CRR-004: If persisting any process, audio-session, or extension signal fails,
  the live session must return failure before reading another signal batch.

## Exclusions

Endpoint notification registration, detector candidate ownership, manual record
semantics, and collection of Windows T2 evidence are outside this change.
