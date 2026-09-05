---
change: change-20260904-phase1-detector-candidate-lifecycle
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

Treat the tab and microphone fields already present on `Candidate` as independently
owned evidence slots. Teardown dispatches by signal kind and matching tree root, clears
only the applicable slot, then rebuilds aggregate evidence and removes an empty
candidate. Active-meeting teardown remains unchanged.
