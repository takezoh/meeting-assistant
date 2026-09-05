---
change: change-20260904-phase1-detector-candidate-lifecycle
role: requirements
---

<!-- lifecycle is owned by change.md -->

# Requirements

## Candidate lifecycle

- DCL-001: An end signal clears only candidate evidence whose evidence kind and
  process-tree root match that end signal.
- DCL-002: Clearing one side must retain a still-live opposite side, but must also
  recompute the candidate subject and cited evidence from the retained side.
- DCL-003: Evidence from a stopped tab tree or microphone tree must never be reused
  by a later signal to produce a determinate start.
- DCL-004: A foreign-tree end signal must not clear either side of an unrelated
  candidate.
