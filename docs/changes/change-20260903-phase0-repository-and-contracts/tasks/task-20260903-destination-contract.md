---
id: task-20260903-destination-contract
kind: task
title: destination-contract
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-destination/Cargo.toml
- crates/ma-destination/src/lib.rs
- crates/ma-destination/src/identity.rs
- crates/ma-destination/src/retry.rs
- crates/ma-destination/src/audit.rs
- contracts/destination/destination-descriptor.schema.json
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-workflow-core-contract}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the replaceable export seam, export identity, retry classification and
  the local egress audit.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-destination-contract.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the replaceable export seam, export identity, retry classification and the local egress audit.

## Execution contract

- Output: A Rust crate with a fake destination that can simulate the crash window and authentication failures.
- Tool guidance: Persist the resumable session or external identifier before the create completes; treat the recorded identity as the only discovery mechanism under the drive.file scope.
- Boundaries: Do not implement Google Drive or Notion clients in this phase; never delete or degrade a local artifact because an export failed.

## Acceptance

- a crash between remote creation and identity recording is reconciled by external-identifier lookup and creates no duplicate
- authentication failures are classified as needs-reauthentication rather than retried blindly
- the persistent export queue survives restart and has a declared backlog cap with a surfaced state
- every send appends an audit record containing identifiers and counts only
- a send to a host absent from egress-inventory.toml is rejected before the request and recorded, and the backlog cap of 500 surfaces the dropped export rather than silently refusing work


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit eb646d37437f with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit eb646d37437f with approved mechanical gate
{% /transition %}
