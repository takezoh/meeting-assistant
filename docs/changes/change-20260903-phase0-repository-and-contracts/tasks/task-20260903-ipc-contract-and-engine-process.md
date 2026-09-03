---
id: task-20260903-ipc-contract-and-engine-process
kind: task
title: ipc-contract-and-engine-process
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-ipc/Cargo.toml
- crates/ma-ipc/src/protocol.rs
- crates/ma-ipc/src/method.rs
- crates/ma-ipc/src/event.rs
- crates/ma-ipc/src/transport.rs
- crates/ma-ipc/src/dispatch.rs
- crates/ma-ipc/src/authz.rs
- contracts/ipc/protocol.schema.json
- contracts/ipc/methods.schema.json
- crates/ma-engine/Cargo.toml
- crates/ma-engine/src/main.rs
- crates/ma-engine/src/supervisor.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-persistence-and-artifact-layout}
- {type: dependsOn, target: task-20260903-session-state-machine}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the engine control channel and stand up the engine process as the single
  per-user authority for session state.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-ipc-contract-and-engine-process.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the engine control channel and stand up the engine process as the single per-user authority for session state.

## Execution contract

- Output: A transport-agnostic protocol crate, JSON Schemas with golden fixtures, and an engine binary with a single-instance lock.
- Tool guidance: Drive both sides over an in-memory duplex for protocol tests and use a real named pipe only for the ACL and squat tests.
- Boundaries: Do not implement capture or detection inside the engine binary beyond wiring the seams; do not add any method whose effect is not observable in a subsequent snapshot.

## Acceptance

- Rust types and JSON Schemas round-trip every golden fixture
- a major protocol mismatch refuses the connection with a typed error naming the required version
- a stalled client is either disconnected or detects the sequence gap and re-snapshots, and never renders stale state
- a connection from a different user SID is refused before method dispatch, and a pre-squatted pipe name causes engine exit rather than silent joining
- an update offered while a session is non-terminal leaves the running engine binary in place and applies only after the session terminates


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 8e086e746ae4 with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 8e086e746ae4 with approved mechanical gate
{% /transition %}
