---
id: design-module-boundaries
kind: design
title: Module boundaries
summary: The crate layering, the L4 adapter sink, the capture-path and native-inference
  isolation rules and the literal classes, all enforced by cargo xtask boundary.
status: active
created: '2026-09-03'
scope_type: policy
responsibilities:
- id: RESP-001
  statement: Declare the dependency direction of every workspace crate in boundary.toml
    and reject any edge that violates it.
- id: RESP-002
  statement: Keep the capture path unreachable from the workflow, processor, destination
    and adapter crates, transitively and through feature gates.
- id: RESP-003
  statement: Confine native inference libraries and C/C++ build scripts to the processor
    host and its adapters.
- id: RESP-004
  statement: Keep every service identifier inside the L4 adapter crates and their
    tables.
- id: RESP-005
  statement: Name the L5 composition root as the only place where the platform collectors,
    the capture engine, the extension channel and the adapter crates are linked together,
    with every adapter dependency renamed so no service identifier appears outside
    L4.
invariants:
- id: INV-001
  statement: A crate depends only on strictly lower layers; L4 adapters depend only
    on L0 and L1 and are depended on only by L5 composition roots (v-boundary-clean-workspace,
    cargo xtask boundary).
  enforcement: test
- id: INV-002
  statement: No capture-path crate reaches ma-workflow, ma-processor, ma-destination,
    ma-store or any adapter crate, and the enforced source list covers every capture-path
    crate including ma-signals-windows and ma-ext-channel (v-isolation-capture-path-edges,
    v-isolation-negative-fixture, v-win1-capture-path-sources-cover-collectors).
  enforcement: test
- id: INV-003
  statement: No crate outside ma-processor-host and ma-processor-* links a native
    inference library or a C/C++ build script into the capture path (v-isolation-native-link-confined).
  enforcement: test
- id: INV-004
  statement: Service identifier tokens and literals appear only in L4 crates and their
    fixtures (v-boundary-clean-workspace, cargo xtask boundary).
  enforcement: test
- id: INV-005
  statement: ma-detect imports none of std::time, std::fs, std::net, std::process,
    rand or HashMap (v-detect-purity-lint).
  enforcement: test
boundaries:
  provides:
  - boundary.toml as the single source of truth for layers, restricted edges, forbidden
    imports, literal classes and isolation rules
  - cargo xtask boundary as the check every CI run and every developer runs
  consumes:
  - cargo metadata --all-features as the crate graph
  - the source tree for import and literal scans
  forbidden:
  - an exception list that names a crate rather than a rule
  - a check that resolves default features only or direct edges only
variability:
  fixed:
  - the six layers L0 to L5 and the L4 sink rule
  - the two isolation rule classes
  free:
  - which external crates count as native inference bindings (the list in boundary.toml)
  - the heuristic that detects a C/C++ build script
capabilities:
- id: cap:dependency-direction-enforcement
  uniqueness: global
failure_responsibilities:
- id: FR-001
  statement: A violation names the edge, the rule and the layer pair; the check exits
    non-zero and CI blocks the merge.
- id: FR-002
  statement: A false positive is corrected by refining the rule in boundary.toml or
    the heuristic in xtask, never by an ad hoc exemption in a crate.
trust_boundaries:
- id: TB-001
  statement: 'workspace source to policy: the check reads every crate''s manifest
    and source and trusts none of it; a crate cannot declare itself exempt.'
compatibility_policies:
- id: CP-001
  statement: Adding a crate requires a layer assignment in boundary.toml or a matching
    layer pattern; an unassigned crate is a violation.
- id: CP-002
  statement: Moving a responsibility between layers is a boundary.toml change reviewed
    with the ADR that motivates it.
tags:
- architecture
- boundaries
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- boundary.toml
- xtask/src/boundary.rs
---

## Purpose

The workspace is a set of crates whose dependency direction is the architecture. This document states the
layering and the isolation rules that `cargo xtask boundary` enforces, so that a reader can predict which
crate may know about which without reading twenty manifests.

## Responsibilities

`boundary.toml` owns the layer table, the restricted-edge rule for the L4 sink, the forbidden-import list for
the detector, the two literal classes and the two isolation rules. `xtask/src/boundary.rs` owns the check
over `cargo metadata --all-features`, the source scan for imports and literals, and the native-linking
heuristic (a `links` key plus a C/C++ build dependency).

## Boundaries

| Layer | Crates | May depend on |
| --- | --- | --- |
| L0 | `ma-core-types` | nothing in the workspace |
| L1 | `ma-signal`, `ma-ipc`, `ma-processor`, `ma-destination`, `ma-manifest`, `ma-secure` | L0 |
| L2 | `ma-session`, `ma-detect`, `ma-workflow` | L0, L1 |
| L3 | `ma-store`, `ma-capture`, `ma-signals-windows`, `ma-ext-channel` | L0 to L2 |
| L4 | `ma-adapter-*`, `ma-processor-*`, `ma-destination-*` | L0, L1 only |
| L5 | `ma-engine`, `ma-processor-host`, `app-ui`, `xtask` | anything |

L4 is a sink: only L5 composition roots may depend on it. Two consequences shaped Phase 0: the adapter
seam (`MeetingAdapter`, `TableAdapter`, the conformance suite) lives in `ma-signal` so that L4 adapters can
implement it, and `ma-ipc`, `ma-workflow` and `ma-destination` reach persistence and session semantics only
through ports (`SessionAuthority`, `WorkflowStore`, `ExportStore`) that the engine implements.

## Invariants

The five invariants above are each a rule of `boundary.toml` with a named check. The negative fixture
workspace under `xtask/tests/fixtures` plants one violation per rule and the test asserts exactly those ids,
so a check that silently stopped detecting would fail its own test.

## Collaboration

`contract-processing-isolation` relies on INV-002 and INV-003 as its first layer; the child-process boundary
in `ma-processor-host` is its second. `contract-detector-determinism` relies on INV-005 for the detector's
purity. `contract-egress-inventory` is a sibling check over the same source tree with its own owner.

## Failure Responsibility

The check exits non-zero with every violation named. There is no warning mode: a violation is a build
failure in the portable CI tier.

## Variability

Fixed: the layers, the sink rule and the isolation rule classes. Free: the native-crate list and the
build-script heuristic, both of which have been refined twice during Phase 0 against false positives.

## Conformance

`cargo xtask boundary` (T0 in the portable tier), `cargo test -p xtask` for the negative fixtures.

## Related Decisions

adr-20260903-workspace-boundary-enforcement, adr-20260903-capture-engine-process-isolation,
adr-20260903-workflow-runtime-process-topology.
