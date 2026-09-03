---
id: task-20260903-persistence-and-artifact-layout
kind: task
title: persistence-and-artifact-layout
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-store/Cargo.toml
- crates/ma-store/src/schema.rs
- crates/ma-store/src/migration.rs
- crates/ma-store/migrations/*.sql
- crates/ma-store/src/repo/**
- crates/ma-store/src/purge.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-core-types-and-identity}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Define the relational state, its migration discipline, writer ownership and
  the relocatable artifact addressing model.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-persistence-and-artifact-layout.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Define the relational state, its migration discipline, writer ownership and the relocatable artifact addressing model.

## Execution contract

- Output: A Rust crate with embedded SQL migrations and repository modules, plus tests against temporary directories.
- Tool guidance: Configure WAL, busy_timeout and foreign keys explicitly and assert the configuration in a test; use BEGIN IMMEDIATE for read-modify-write.
- Boundaries: Do not implement workflow or export logic here; do not store any absolute path; do not make the database path configurable.

## Acceptance

- no inserted row contains an absolute artifact path or a drive or UNC prefix
- relocating the artifact root updates one row and leaves every reference resolvable
- a write to a table outside the connection role's family is rejected
- migrations apply forward from every released version and a newer database is refused with a typed error
- deleting a meeting hides it in one transaction and, after the purge job runs, the meeting_id appears nowhere under the artifact root and in no row outside tombstone
- a purge killed mid-walk resumes on restart and a second run is a no-op returning success


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 934ec980f35b with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 934ec980f35b with approved mechanical gate
{% /transition %}
