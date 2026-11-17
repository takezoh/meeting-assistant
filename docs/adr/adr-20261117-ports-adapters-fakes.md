---
id: adr-20261117-ports-adapters-fakes
kind: adr
title: Separate decisions from effects with ports, adapters and fakes
status: accepted
created: '2026-11-17'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/core/src/ports.rs
- crates/core/src/fakes.rs
- crates/core/tests/architecture.rs
- crates/app/src/architecture.rs
- crates/app/src/composition.rs
summary: Decisions are pure or depend only on ports; every port has an adapter and
  a fake; platform code is small operations.
updated: '2026-11-17'
---

# Separate decisions from effects with ports, adapters and fakes

## Context

The first Rust version was written to get recording working. Core read the file system and the clock directly, the extension link kept sockets, state and files in one module, the install procedure was duplicated per operating system, and the timing and mixing calculations sat inside Windows-specific code. Logic could only be exercised by running the real app, and adding macOS meant rewriting procedures, not just operations.

The user directed that the implementation be structured by responsibility, separate pure functions from side effects, implement ports, adapters and fakes, and keep platform-dependent code as small ports.

## Decision

Deciding code is pure or depends only on ports. Each port has a real adapter and a fake. Platform code implements small operations; procedures and policy are shared. Only the composition root names concrete types. Two tests that read the source code enforce the dependency rules.

The governing rules are kept in the [effect boundaries design](../design/design-effect-boundaries.md).

## Consequences

- Positive: the coordinator, engine, pipeline, hub and install procedure are tested without devices, browsers or a specific operating system.
- Positive: supporting another platform means implementing listed operations, with the procedures already tested.
- Negative: more types and indirection than a direct implementation, and generic parameters on the engine and recorders.
- Negative: the fakes are code that must be kept faithful to the adapters; a fake that is more permissive than the real system hides defects.
- Neutral: the architecture tests match text in source files, so they need updating when modules move.

## Confirmation

`crates/core/tests/architecture.rs` and `crates/app/src/architecture.rs` fail when a forbidden dependency appears. `cargo test --workspace` runs every decision path against fakes.


{% transition from="proposed" to="accepted" date="2026-11-17" %}
Decided by the user on 2026-11-17
{% /transition %}
