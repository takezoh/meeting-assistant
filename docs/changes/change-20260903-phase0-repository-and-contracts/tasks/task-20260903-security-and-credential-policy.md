---
id: task-20260903-security-and-credential-policy
kind: task
title: security-and-credential-policy
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-secure/Cargo.toml
- crates/ma-secure/src/secret.rs
- crates/ma-secure/src/credential_store.rs
- crates/ma-secure/src/redaction.rs
- crates/ma-secure/src/acl.rs
- docs/design/threat-model.md
- docs/design/credential-policy.md
- egress-inventory.toml
- crates/ma-secure/tests/egress_inventory.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-core-types-and-identity}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix secret custody, log redaction, ACL construction and the documented threat
  model and trust boundaries.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-security-and-credential-policy.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix secret custody, log redaction, ACL construction and the documented threat model and trust boundaries.

## Execution contract

- Output: A Rust crate with compile-fail tests and a leak-scanning system test, plus two design documents.
- Tool guidance: Make redaction a type-level property rather than a logging convention; scan the whole written-file set for planted markers rather than inspecting selected logs.
- Boundaries: Do not add a secondary secret cache anywhere; do not pass secrets through process arguments.

## Acceptance

- a planted secret and planted meeting content appear in no file the application writes, including panic output and parse errors
- the secret type provably cannot be displayed or serialized in raw form, enforced by a compile-fail test
- constructed pipe and file security descriptors grant the owning user only
- a missing credential produces a typed needs-authentication result with the feature disabled and surfaced
- every host reachable from workspace source or from a contracts/ manifest appears in egress-inventory.toml, every entry declares a closed integration_owner, and both an undeclared host and a stale entry fail with distinct codes


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 0c895e2962bf with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 0c895e2962bf with approved mechanical gate
{% /transition %}
