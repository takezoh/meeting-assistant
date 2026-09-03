---
id: task-20260903-ui-shell-consent-surface
kind: task
title: ui-shell-consent-surface
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- app/ui/src-tauri/Cargo.toml
- app/ui/src-tauri/src/main.rs
- app/ui/src-tauri/src/engine_client.rs
- app/ui/src-tauri/tauri.conf.json
- app/ui/src/**
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-ipc-contract-and-engine-process}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Provide the consent and visibility surface as a thin client of the engine,
  owning no session truth.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-ui-shell-consent-surface.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Provide the consent and visibility surface as a thin client of the engine, owning no session truth.

## Execution contract

- Output: A Tauri 2 application skeleton with the engine client factored into a testable Rust module.
- Tool guidance: Keep all reconnect and resync logic in the Rust client module so it is covered by headless tests.
- Boundaries: Do not derive session state in the frontend; do not implement the meeting library, playback or settings beyond what the consent surface requires.

## Acceptance

- the UI renders only engine-supplied state and re-snapshots after any disconnect or sequence gap
- the countdown and its cancel affordance are driven by engine events, not by a local timer
- the client declares indicator and cancel capabilities at handshake, and automatic recording still starts when no client is running at all
- the engine client library is testable headlessly without WebView2


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit fbe08c396bdb with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit fbe08c396bdb with approved mechanical gate
{% /transition %}
