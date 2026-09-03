---
id: task-20260903-workspace-and-boundary-scaffold
kind: task
title: workspace-and-boundary-scaffold
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- Cargo.toml
- rust-toolchain.toml
- boundary.toml
- verification-tiers.toml
- deny.toml
- xtask/Cargo.toml
- xtask/src/main.rs
- xtask/src/boundary.rs
- xtask/src/verify.rs
- xtask/tests/boundary_negative.rs
- xtask/tests/isolation_negative.rs
- xtask/tests/fixtures/violating-workspace/**
- .github/workflows/ci.yml
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Create the cargo workspace with declared layers, make the module boundary
  and processing-isolation rules mechanically enforced and provably non-vacuous, and
  make verification tiering a build-time obligation rather than a convention.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-workspace-and-boundary-scaffold.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Create the cargo workspace with declared layers, make the module boundary and processing-isolation rules mechanically enforced and provably non-vacuous, and make verification tiering a build-time obligation rather than a convention.

## Execution contract

- Output: Rust workspace manifests, a declarative boundary.toml and verification-tiers.toml, an xtask binary with boundary and verify subcommands, a fixture workspace carrying violations and decoys, and a CI workflow with two required jobs.
- Tool guidance: Resolve the graph from cargo metadata with all features enabled; check transitive edges, not only direct ones; tokenize sources so identifiers, string literals and comments are distinguishable rather than grepping raw bytes; report all violations in one pass rather than stopping at the first.
- Boundaries: Do not implement any product behaviour; do not add crates beyond empty layer placeholders needed for the graph; do not weaken the negative fixture to make the checker pass.

## Acceptance

- cargo xtask boundary exits 0 on the clean workspace and reports the number of edges checked
- the negative fixture produces a non-zero exit naming exactly the planted forbidden edge, forbidden literal and forbidden import, and reports none of the three planted decoys (a doc comment containing 'must meet', a graph_edge binding, a "meeting ended" literal)
- a feature-gated dependency from a non-composition-root crate onto an adapter crate is detected, and so are a capture-path edge onto ma-workflow and a native-inference dependency on ma-engine
- cargo xtask verify --check-registration fails when a T2 verification id is absent from verification-tiers.toml, and CI defines both the portable and the windows required jobs


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit cd8539782f3b with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit cd8539782f3b with approved mechanical gate
{% /transition %}
