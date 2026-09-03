---
id: task-20260903-session-state-machine
kind: task
title: session-state-machine
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-session/Cargo.toml
- crates/ma-session/src/state.rs
- crates/ma-session/src/transition_table.rs
- crates/ma-session/src/mode.rs
- crates/ma-session/src/deadline.rs
- contracts/session/transitions.json
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-core-types-and-identity}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the meeting-session lifecycle, the automatic recording mode policy, deadline
  semantics and the consent precondition as a pure, exhaustively testable function.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-session-state-machine.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the meeting-session lifecycle, the automatic recording mode policy, deadline semantics and the consent precondition as a pure, exhaustively testable function.

## Execution contract

- Output: A pure Rust crate plus a JSON transition table used as the conformance source of truth.
- Tool guidance: Pass time in as an argument rather than reading a clock; keep every effect a returned value rather than a side effect.
- Boundaries: Do not perform I/O, start capture, or talk to the store or IPC from this crate; do not encode any service-specific knowledge.

## Acceptance

- the exported transition table equals contracts/session/transitions.json
- step is total over the state and event space, returning Rejected where no transition is declared
- a suspend and resume spanning a countdown re-evaluates instead of firing
- an automatic start decision with no consent surface of either kind - no deliverable engine notification and no attached client - produces a suppression record and no capture effect, while a deliverable notification alone is sufficient to arm
- no audio sample is written under the artifact root while the session is in candidate or arming, and a cancelled countdown leaves the meeting directory with zero chunk files
- a detection in ask mode returns a notify effect whose action set carries start, so ask mode is satisfiable with no client attached, and returns a suppression with cause no_consent_surface when neither surface can present it


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 7a8a700f6e7e with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 7a8a700f6e7e with approved mechanical gate
{% /transition %}
