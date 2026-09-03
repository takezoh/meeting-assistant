---
id: adr-20260903-workspace-boundary-enforcement
kind: adr
title: Cargo workspace with declared layers and a mechanically enforced boundary policy
summary: Layer membership, allowed edges, sink crates, forbidden imports and a two-class
  literal scan live in boundary.toml and are enforced by cargo xtask boundary with
  a decoy-carrying negative fixture.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - The leak-proof rule is decided by cargo metadata rather than by reviewer attention,
    and blocks merge on failure.
  - The negative fixture proves the checker can both detect violations and not report
    decoys, so neither failure mode is silent.
  - PLAN section 7's reliability guarantee gains a build-time component rather than
    existing only as prose.
  negative:
  - boundary.toml is a second place where structure is declared and can drift from
    intent if it is edited to make a failure go away.
  - The scan surface requires a tokenizer rather than a text search, which is more
    work than a grep.
  - Adding a legitimate new vendor literal requires editing the policy file, which
    is friction by design but is still friction.
  neutral:
  - The checker is an xtask binary, so it is versioned and tested with the repository
    rather than installed separately.
  - cargo-deny covers advisories, licences and banned crates in the same continuous-integration
    job.
confirmation: cargo xtask boundary (T0) on the clean workspace; cargo test -p xtask
  boundary_negative_fixture_reports_three_violations (T1) and feature_gated_adapter_edge_is_detected
  (T1).
tags:
- architecture
- boundaries
- ci
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

"Meeting-service-specific logic cannot leak into the workflow core" is a Phase 0 exit criterion. An exit criterion that is checked by review is not an exit criterion; it is an intention. The check also has to be provably non-vacuous, because the most likely way this deliverable fails silently is a checker that always reports success.

A second constraint appeared during integration: PLAN section 7's guarantee that processing failure never stops recording is also a graph property, and belongs to the same enforcement mechanism.

## Decision

The repository is a cargo workspace with layers declared in `boundary.toml`: L0 kernel, L1 contracts, L2 domain, L3 infrastructure, L4 adapters, L5 composition roots. A crate may depend only on strictly lower layers. **L4 is a sink**: nothing but an L5 composition root may depend on an adapter crate. Two further rule classes carry the processing-isolation guarantee: the capture-path crates may not reach the workflow, processor or destination crates, and only the processor host and its adapters may link a native inference library.

`cargo xtask boundary` resolves the graph from `cargo metadata --all-features`, so a feature-gated dependency cannot hide a leak, and checks transitive edges rather than direct ones only.

The literal scan has a **declared surface**, because two checkers that both pass the fixtures can otherwise disagree on real source. Class A matches word-split identifier tokens (crate names, path segments, item names, bindings) against a list of service words. Class B matches whole string literals against a table of process, package and host names. Comments and doc comments are never scanned by either class, and substring matching is forbidden. The words `meet`, `edge` and `chrome` are deliberately absent from class A: they are ordinary words in ordinary code, and a checker that fails on `graph_edge` or on "the requirement this adapter must meet" produces false positives whose only relief is widening the allowlist.

`xtask/tests/boundary_negative.rs` runs the checker against a fixture workspace carrying three planted violations **and three planted decoys**, and asserts the exact violation set — no more and no fewer.

## Alternatives considered

**Review-only enforcement with documented layering.** Rejected because the exit criterion becomes unfalsifiable, and layering decays at exactly the moments when it matters most.

**Crate-level separation without a checker.** Rejected because Rust's module system does not prevent adding a dependency, and a feature-gated dependency is invisible in a default-features graph.

**A raw-text grep for vendor names.** Simple and was the drafts' implied surface. Rejected because it fails on English prose and on graph terminology, and the resulting pressure to widen the allowlist is how a gate stops meaning anything.

**Violations-only negative fixture.** Rejected because it proves detection power without proving precision, and a checker with false positives is abandoned rather than fixed.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- The leak-proof rule is decided by cargo metadata rather than by reviewer attention, and blocks merge on failure.
- The negative fixture proves the checker can both detect violations and not report decoys, so neither failure mode is silent.
- PLAN section 7's reliability guarantee gains a build-time component rather than existing only as prose.

**Negative.**

- boundary.toml is a second place where structure is declared and can drift from intent if it is edited to make a failure go away.
- The scan surface requires a tokenizer rather than a text search, which is more work than a grep.
- Adding a legitimate new vendor literal requires editing the policy file, which is friction by design but is still friction.

**Neutral.**

- The checker is an xtask binary, so it is versioned and tested with the repository rather than installed separately.
- cargo-deny covers advisories, licences and banned crates in the same continuous-integration job.

## Confirmation

cargo xtask boundary (T0) on the clean workspace; cargo test -p xtask boundary_negative_fixture_reports_three_violations (T1) and feature_gated_adapter_edge_is_detected (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
