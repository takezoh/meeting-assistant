---
id: design-effect-boundaries
kind: design
title: Pure decisions and side-effect boundaries
status: active
created: '2026-11-17'
scope_type: policy
responsibilities:
- id: RESP-001
  statement: "Define the dependency direction between deciding code, ports, adapters, platform code and the composition root."
- id: RESP-002
  statement: "Define what a port, an adapter and a fake must provide and how each is verified."
invariants:
- id: INV-001
  statement: "crates/core uses no file system, network, process, environment, thread, clock or OS crate outside its fakes and tests."
  enforcement: test
- id: INV-002
  statement: "OS crates are used only under crates/app/src/platform."
  enforcement: test
- id: INV-003
  statement: "Deciding modules of the app touch no file, socket, process, environment, platform module or adapter."
  enforcement: test
- id: INV-004
  statement: "Both platform implementations expose the same names through platform/mod.rs."
  enforcement: test
- id: INV-005
  statement: "Every port has a fake and the code that depends on the port is tested with it."
  enforcement: review
boundaries:
  provides:
  - id: PROV-001
    statement: "The rules every module of the workspace follows when it needs an effect."
  consumes: []
  forbidden:
  - id: FORB-001
    statement: "A procedure or policy implemented once per operating system inside platform code."
  - id: FORB-002
    statement: "An exception added to an architecture test to let an effect stay in a deciding module."
variability:
  fixed:
  - id: FIX-001
    statement: "Dependency direction: deciding code depends on ports; adapters and platform code implement ports; only the composition root names concrete implementations."
  free:
  - id: FREE-001
    statement: "Module placement, the shape of a port and the internals of adapters and fakes."
capabilities: []
failure_responsibilities:
- id: FAIL-001
  statement: "Adapters report failure as an error with context; deciding code chooses what the failure means for the recording."
trust_boundaries: []
compatibility_policies: []
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-ports-adapters-fakes}
source_paths:
- crates/core/tests/architecture.rs
- crates/app/src/architecture.rs
- crates/core/src/ports.rs
- crates/core/src/fakes.rs
- crates/app/src/composition.rs
- AGENTS.md
summary: Dependency direction and verification rules for pure decisions, ports, adapters, fakes and platform code.
---

## Purpose

Keep the decisions of the app verifiable without a meeting app, a browser, a microphone or a particular operating system, and keep the amount of code that must be written and checked per platform small.

## Responsibilities

Deciding code determines what should happen: whether a meeting has started, what to record, in which order to install, how to pace frames. It is a pure function, or a type that depends only on ports. A port is a trait naming the smallest effect the decision needs. An adapter implements a port against the real world; a fake implements it for tests. Platform code is the subset of adapters that calls operating system APIs. The composition root (`crates/app/src/composition.rs`, `main.rs`) is the only place that chooses concrete implementations.

## Boundaries

`crates/core` knows nothing about files, clocks, threads or operating systems. In `crates/app`, the modules `cli`, `install`, `linked`, `worker`, `browser/protocol`, `browser/hub` and `recording/{plan,timeline,mixer,pipeline}` decide and must stay free of effects; `adapters`, `browser/{server,endpoint,relay}`, `recording/session`, `platform`, `tray` and `main` act.

A platform operation is small: read a list, write a value, open a capture, copy a file. The sequence in which operations are used, the handling of their failure and any policy belong to shared code and are written once.

## Invariants

The invariants in the front matter are enforced by two tests that read the source code: `crates/core/tests/architecture.rs` and `crates/app/src/architecture.rs`. They scan production code only; everything after `#[cfg(test)]` is ignored so that tests may use threads and temporary files to drive fakes.

## Collaboration

A new need for an effect is met by adding an operation to a port, implementing it in the fake, testing the procedure that uses it, and then implementing it per platform. A platform that cannot provide the operation yet returns an error saying so.

## Failure Responsibility

An adapter reports a failure with enough context to diagnose it and does not decide policy. Deciding code states what the failure means: retry, continue degraded, or stop. A fake must be able to produce each failure the decision handles.

## Variability

Ports may be split, merged or renamed, and modules may move, as long as the dependency direction holds and the architecture tests are updated to describe the new layout rather than to permit an effect.

## Conformance

Run `cargo test --workspace`; the architecture tests fail with the file and the forbidden token. Static scanning does not prove the absence of every effect, so review still checks that new deciding code takes time, randomness and I/O as inputs.

## Related Decisions

[ADR: separate decisions from effects with ports, adapters and fakes](../adr/adr-20261117-ports-adapters-fakes.md). Working instructions are in the root `AGENTS.md`, and the suites are described in `docs/technical/testing.md`.
