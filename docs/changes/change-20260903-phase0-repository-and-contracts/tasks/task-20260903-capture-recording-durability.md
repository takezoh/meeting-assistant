---
id: task-20260903-capture-recording-durability
kind: task
title: capture-recording-durability
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-capture/Cargo.toml
- crates/ma-capture/src/source.rs
- crates/ma-capture/src/chunk_writer.rs
- crates/ma-capture/src/manifest.rs
- crates/ma-capture/src/recovery.rs
- crates/ma-capture/src/consolidate.rs
- contracts/artifact/chunk-manifest.schema.json
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-persistence-and-artifact-layout}
- {type: dependsOn, target: task-20260903-ipc-contract-and-engine-process}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Make durable recording, honest recovery and lossless consolidation real and
  testable behind a synthetic capture source.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-capture-recording-durability.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Make durable recording, honest recovery and lossless consolidation real and testable behind a synthetic capture source.

## Execution contract

- Output: A Rust crate with a CaptureSource seam, a deterministic synthetic source, and integration tests that kill processes.
- Tool guidance: Order durability as flush, rename, manifest append, fsync; use a fault-injecting filesystem fake for backpressure and disk-full paths.
- Boundaries: Do not implement WASAPI or any real device access; do not let the writer touch the database, IPC or the network.

## Acceptance

- killing the engine mid-chunk loses at most the in-progress chunk and recovery repairs or gaps the partial file
- the chunk directory is treated as truth and the manifest is reconciled in both directions
- a stalling filesystem produces an explicit gap and a degraded event rather than stalling the capture callback
- consolidated FLAC decodes sample-identically before any chunk is deleted, and a crash between verification and deletion re-runs idempotently
- aborting a scripted processor host child mid-job during a synthetic recording leaves chunk cadence unchanged and the session in recording


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 074f65c8bbed with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 074f65c8bbed with approved mechanical gate
{% /transition %}
