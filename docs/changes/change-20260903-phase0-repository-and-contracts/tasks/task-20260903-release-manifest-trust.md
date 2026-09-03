---
id: task-20260903-release-manifest-trust
kind: task
title: release-supply-chain
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-manifest/Cargo.toml
- crates/ma-manifest/src/manifest.rs
- crates/ma-manifest/src/verify.rs
- crates/ma-manifest/src/rollback.rs
- crates/ma-manifest/src/keys.rs
- contracts/manifest/update-manifest.schema.json
- contracts/manifest/adapter-manifest.schema.json
- .github/workflows/release.yml
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-security-and-credential-policy}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the signed update and adapter manifest trust model, including rollback
  protection and key rotation, with no server-side trust decision.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-release-manifest-trust.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the signed update and adapter manifest trust model, including rollback protection and key rotation, with no server-side trust decision.

## Execution contract

- Output: A Rust crate taking bytes and a key set as arguments, JSON Schemas, and a release workflow.
- Tool guidance: Verify first and parse second so an unverified document cannot influence control flow; test the captive-portal HTML response case explicitly.
- Boundaries: Do not introduce any first-party service for update metadata or token exchange; do not auto-approve downgrades.

## Acceptance

- tampered, downgraded, unknown-key and digest-mismatched manifests are all rejected with distinct typed codes
- no manifest-declared value is used, including in logs, before verification succeeds
- a key rollover block signed by the current key introduces the next key and an unknown-key-only manifest is refused
- an engine replacement is deferred while any session is non-terminal


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 1371018a654c with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 1371018a654c with approved mechanical gate
{% /transition %}
