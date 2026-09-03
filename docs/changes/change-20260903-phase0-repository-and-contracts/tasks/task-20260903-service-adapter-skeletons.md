---
id: task-20260903-service-adapter-skeletons
kind: task
title: service-adapter-skeletons
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-adapter-teams/**
- crates/ma-adapter-slack/**
- crates/ma-adapter-zoom/**
- crates/ma-adapter-meet/**
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-signal-and-detector-contracts}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Establish the adapter seam and four service-specific data-only adapters that
  hold every service identifier.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-service-adapter-skeletons.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Establish the adapter seam and four service-specific data-only adapters that hold every service identifier.

## Execution contract

- Output: Four small Rust crates implementing one trait, with a shared conformance test suite.
- Tool guidance: Keep each adapter a declarative table plus a match function; do not let adapters depend on each other.
- Boundaries: Do not add detection heuristics or version-specific workarounds beyond placeholders; do not import platform APIs here.

## Acceptance

- each adapter crate is a graph sink depended on only by composition roots
- a shared adapter conformance suite passes for all four adapters
- a panicking adapter is disabled with a diagnostic and does not fail the detection pipeline
- all service identifiers appear only inside these crates and their fixtures


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 6922d6596ff9 with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 6922d6596ff9 with approved mechanical gate
{% /transition %}
