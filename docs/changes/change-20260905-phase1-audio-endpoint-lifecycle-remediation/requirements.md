---
change: change-20260905-phase1-audio-endpoint-lifecycle-remediation
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Lifecycle requirements

- AEL-001: Session callbacks queued before endpoint removal must be delivered in
  callback order; endpoint teardown must not erase them.
- AEL-002: Every active endpoint registration is health-checked. A stale manager
  is unregistered and recreated from the current device even when the device id
  is unchanged.
- AEL-003: Teardown unregisters the endpoint notification and all session-event
  sinks owned by that endpoint, then removes their known-session keys.
- AEL-004: Portable tests must exercise the same refresh planning, registration
  teardown, and notification merge helpers used by the live Windows path.
