---
id: task-20260903-workflow-core-contract
kind: task
title: workflow-core-contract
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-workflow/Cargo.toml
- crates/ma-workflow/src/step.rs
- crates/ma-workflow/src/queue.rs
- crates/ma-workflow/src/retry.rs
- crates/ma-workflow/src/lifecycle.rs
- crates/ma-workflow/src/edits.rs
- crates/ma-workflow/src/effect_ledger.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-persistence-and-artifact-layout}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix step identity, idempotency, retry classification, artifact lifecycle
  and the separation of generated content from user edits.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-workflow-core-contract.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix step identity, idempotency, retry classification, artifact lifecycle and the separation of generated content from user edits.

## Execution contract

- Output: A Rust crate driven in tests by recording fake processors and destinations.
- Tool guidance: Commit the effect ledger's intended row before any effect outside the state database and update it to applied afterwards; compose generation plus overlay at read time rather than materialising the merged text; decompose transcription into per-chunk work items with stable identifiers.
- Boundaries: Do not implement any processor or destination here; do not let a processing failure reach the capture path.

## Acceptance

- enqueueing a completed step key returns the recorded result and executes nothing
- a lease-expired running step is re-run without producing a duplicate artifact
- changing processor version or configuration produces a new step and retains the previous result
- an effect ledger row left at intended by a kill is resolved by lookup or by an explicit user decision on restart, never by a silent recreate
- regeneration adds a generation row and never mutates edit_overlay; an edit whose anchor is gone is retained with orphaned = true and is enumerable, and an edit offered with no anchor basis is refused


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit db57fb2ca985 with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit db57fb2ca985 with approved mechanical gate
{% /transition %}
