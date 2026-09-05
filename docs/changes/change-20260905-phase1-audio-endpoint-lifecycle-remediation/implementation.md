---
change: change-20260905-phase1-audio-endpoint-lifecycle-remediation
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

## Responsibility boundary

Extract pure endpoint refresh planning from known, active, and unhealthy endpoint
sets. Use that plan in `WindowsSessionManager::refresh_endpoints`; manager health is
defined by successful session enumeration. Extract session registration removal
and notification merge into portable helpers, leaving COM unregister calls in the
Windows adapter.

Do not add polling-derived microphone transitions or a second notification source.
