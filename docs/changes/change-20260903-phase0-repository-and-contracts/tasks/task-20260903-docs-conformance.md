---
id: task-20260903-docs-conformance
kind: task
title: docs-and-adr-materialization
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- docs/adr/**
- docs/design/module-boundaries.md
- docs/design/session-lifecycle.md
- docs/design/recording-artifact-model.md
- docs/design/threat-model.md
- docs/design/credential-policy.md
- docs/changes/change-20260903-phase0-repository-and-contracts/change.md
- docs/changes/change-20260903-phase0-repository-and-contracts/requirements.md
- docs/changes/change-20260903-phase0-repository-and-contracts/implementation.md
- docs/changes/change-20260903-phase0-repository-and-contracts/verification.md
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-workspace-and-boundary-scaffold}
- {type: dependsOn, target: task-20260903-session-state-machine}
- {type: dependsOn, target: task-20260903-signal-and-detector-contracts}
- {type: dependsOn, target: task-20260903-capture-recording-durability}
- {type: dependsOn, target: task-20260903-processor-contract}
- {type: dependsOn, target: task-20260903-destination-contract}
- {type: dependsOn, target: task-20260903-security-and-credential-policy}
- {type: dependsOn, target: task-20260903-release-manifest-trust}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Record Phase 0's decisions and durable discipline in the repository's documentation
  system without duplicating the same content across ADR, design and change package.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-docs-conformance.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Record Phase 0's decisions and durable discipline in the repository's documentation system without duplicating the same content across ADR, design and change package.

## Execution contract

- Output: Markdown documents with schema-conformant frontmatter.
- Tool guidance: Let ADRs own the reasons, design documents own the current discipline, and the change package own the generation context; validate with the dev-docs conformance tooling before closing.
- Boundaries: Do not set any ADR to accepted; do not move the change status past its whitelisted next state; do not copy implementation file inventories into persistent design documents.

## Acceptance

- every ADR sits at docs/adr/adr-YYYYMMDD-slug.md, declares non-empty decision_makers, uses the tripolar consequences object with all three lists non-empty, and starts at proposed
- exactly five design documents exist - module-boundaries, session-lifecycle, recording-artifact-model, threat-model and credential-policy - each validating against design.schema.json with every invariant naming a mechanical check where one exists
- the three required change members are materialized and non-empty
- the change root promotion manifest declares none with a reason, and no promotion entry names a target, because promotion upserts stable items of an existing design document and this change creates the repository's first ones


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 01bc28de8572 with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 01bc28de8572 with approved mechanical gate
{% /transition %}
