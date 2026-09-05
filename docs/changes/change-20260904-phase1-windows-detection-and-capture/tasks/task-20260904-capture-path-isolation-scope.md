---
id: task-20260904-capture-path-isolation-scope
kind: task
title: boundary-capture-path-scope
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- boundary.toml
- xtask/tests/boundary_policy.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Extend the capture-path-isolation rule's source list to match module-boundaries.md
  INV-002 and make the completeness of that list mechanically checked.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-capture-path-isolation-scope.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Extend the capture-path-isolation rule's source list to match module-boundaries.md INV-002 and make the completeness of that list mechanically checked.

## Execution contract

- Output: A boundary.toml sources-list edit and one xtask test asserting list completeness.
- Tool guidance: Do not add a new boundary rule, forbidden target, literal class or layer assignment; extend the existing rule's sources list only.
- Boundaries: Does not change xtask/src/boundary.rs logic and does not touch the native-inference rule.

## Acceptance

- Given boundary.toml with ma-signals-windows and ma-ext-channel added to [rules.capture-path-isolation].sources, cargo xtask boundary --rule capture-path-isolation stays green on the clean workspace and the negative fixture still reports exactly its planted violations.
- Given the xtask test, the rule's sources list is asserted to contain every crate the design documents call a capture-path crate, so shortening the list is a failure rather than a silently smaller scan.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit be80612af32b on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit be80612af32b on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
