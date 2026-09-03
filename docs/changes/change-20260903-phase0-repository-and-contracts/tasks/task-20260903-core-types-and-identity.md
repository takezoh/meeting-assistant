---
id: task-20260903-core-types-and-identity
kind: task
title: core-types-and-identity
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-core-types/Cargo.toml
- crates/ma-core-types/src/id.rs
- crates/ma-core-types/src/timeline.rs
- crates/ma-core-types/src/artifact_ref.rs
- crates/ma-core-types/src/error.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the shared vocabulary of identifiers, session timeline arithmetic, artifact
  references and errors that every other crate depends on.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-core-types-and-identity.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the shared vocabulary of identifiers, session timeline arithmetic, artifact references and errors that every other crate depends on.

## Execution contract

- Output: A dependency-free Rust library crate with property tests.
- Tool guidance: Use proptest for the tiling and ordering invariants; keep the crate free of platform and I/O dependencies.
- Boundaries: Do not add persistence, capture or IPC concerns here; do not introduce types that only one component uses.

## Acceptance

- identifier types are UUIDv7, time-ordered, and serialize identically in database, path and payload contexts
- chunks and gaps provably tile each track range with no overlap under a property test
- a timestamp computed after a missing chunk retains its true session position
- track descriptors carry capture_mode and contamination_risk


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 7e44557187dc with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 7e44557187dc with approved mechanical gate
{% /transition %}
