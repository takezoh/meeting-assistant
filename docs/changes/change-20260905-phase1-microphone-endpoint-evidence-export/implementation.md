---
change: change-20260905-phase1-microphone-endpoint-evidence-export
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

## Responsibility boundary

The worker shares only the cloned `EndpointSelection` snapshot, not the source or
backend. A platform-neutral conversion produces the sidecar schema. The Windows
recording loop persists the final snapshot after joining the microphone worker and
before stopping the diagnostic session.
