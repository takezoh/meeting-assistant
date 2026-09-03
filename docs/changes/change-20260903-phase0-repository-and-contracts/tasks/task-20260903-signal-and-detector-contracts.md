---
id: task-20260903-signal-and-detector-contracts
kind: task
title: signal-and-detector-contracts
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-signal/Cargo.toml
- crates/ma-signal/src/envelope.rs
- crates/ma-signal/src/source.rs
- crates/ma-signal/src/timeline.rs
- contracts/signal/signal-envelope.schema.json
- crates/ma-detect/Cargo.toml
- crates/ma-detect/src/detector.rs
- crates/ma-detect/src/adapter.rs
- crates/ma-detect/src/decision.rs
- crates/ma-detect/src/outcome.rs
- fixtures/signal-timelines/**
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-core-types-and-identity}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix what a signal is, how timelines are recorded and replayed, and make the
  detector a pure evidence-citing function with a closed outcome partition.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-signal-and-detector-contracts.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix what a signal is, how timelines are recorded and replayed, and make the detector a pure evidence-citing function with a closed outcome partition.

## Execution contract

- Output: Two Rust crates, a JSON Schema, and a set of committed replayable timeline fixtures.
- Tool guidance: Enforce detector purity through the boundary check's forbidden-import list rather than review; use ordered collections or explicit sorts everywhere a decision order is observable.
- Boundaries: Do not implement Windows collectors or detection heuristics here; do not put any service name in either crate.

## Acceptance

- the signal schema contains no free-text subject field capable of carrying UI-derived text
- replaying a fixture yields byte-identical decisions across repeated runs and a fresh process
- every decision cites at least one signal identifier and a rule identifier
- the outcome partition is exhaustive and an extension-authority signal alone never yields a determinate start


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 3b6a243a59c8 with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 3b6a243a59c8 with approved mechanical gate
{% /transition %}
