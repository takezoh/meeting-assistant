---
change: change-20260905-phase1-microphone-endpoint-evidence-export
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Evidence requirements

- MEE-001: The microphone worker must publish the initial opened endpoint and every
  successful successor endpoint in order.
- MEE-002: The exported record must also state coalesced hints and failed switches.
- MEE-003: On worker completion, `ma-diag` must write and fsync a JSON sidecar next
  to the session timeline; a write failure makes the command fail.
- MEE-004: System-default selection is represented by a concrete stable string,
  never by null.
