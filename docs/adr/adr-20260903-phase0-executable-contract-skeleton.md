---
id: adr-20260903-phase0-executable-contract-skeleton
kind: adr
title: Phase 0 delivers executable, checkable contracts and a two-tier verification
  regime
summary: Phase 0 ships type crates, JSON Schemas, synthetic seams, boundary lint and
  CI rather than documents alone, with verification split into a portable tier and
  a registered Windows tier.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Each Phase 0 exit criterion has a check that fails when the criterion is violated.
  - A test that needs Windows cannot exist unregistered and therefore cannot silently
    never run.
  - The contract-core crates are provably free of platform coupling from day one,
    which the later macOS phase inherits.
  - Schema and type drift is impossible rather than merely discouraged.
  negative:
  - Phase 0 grows a test suite, a continuous-integration pipeline and a hard dependency
    on a Windows 11 runner, without which the phase cannot be completed.
  - Synthetic seams exist only to make Phase 0 checkable and must be maintained until
    the real implementations land.
  - Adding a T2 test now requires a tier-file edit, which is friction on every future
    durability test.
  neutral:
  - The tier file becomes a second inventory of verifications alongside the plan,
    kept honest by the registration check.
  - ADRs remain at proposed until a decision maker accepts them; acceptance is a gate
    before the implementing units start.
confirmation: cargo xtask verify --tier portable (T0) on a non-Windows host, --check-registration
  (T0), --tier windows (T2); cargo test -p xtask ci_defines_portable_and_windows_gates
  (T0); dev-docs conformance --root docs (T0).
tags:
- process
- verification
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

Two of the four Phase 0 exit criteria — the capture engine continues recording after the interface terminates, and workflow steps and artifacts have stable identifiers and states — are not decidable from documents. A Phase 0 that ships prose would have to move their verification into Phase 1, where they would compete with feature work and, in practice, never be written.

Shipping tests has its own failure mode. Every one of those tests needs Windows, and if continuous integration gates only the static checks, Phase 0 can be declared complete with a green board while its durability, authorization and consent suites have never executed anywhere. That is not hypothetical; it is the default outcome of writing tests and not saying who runs them.

This ADR is also the single home of the Phase 0 depth decision. Carrying it as both a decision and an open question made every acceptance criterion depend on something the plan simultaneously answered and left open.

## Decision

Phase 0 delivers a **contract-carrying skeleton**: every cross-boundary shape exists twice, as Rust types and as a JSON Schema under `contracts/`, with a conformance test round-tripping golden fixtures so the two cannot drift; behavioural seams have deterministic Phase 0 implementations (a synthetic capture source, a fixture-replay signal source, recording fake processors and destinations) that are the same seams Phase 1 to 4 implement for real; and everything nominal is a policy file with a conformance test rather than scattered code.

Verification is **two declared tiers**, both listed in `verification-tiers.toml`. The `portable` tier runs on any host, including the Linux development host, and holds every T0 and T1 verification plus the build and test of the contract-core crates. The `windows` tier runs on a Windows 11 runner and holds every T2. **Registration is mandatory and checked**: a verification identifier absent from the tier file, or in the portable tier while its test is Windows-annotated, fails the build — so a T2 that nobody registered is a build failure rather than a test that quietly never runs.

Continuous integration defines two required jobs. The portable job runs on every push and pull request and blocks merge. The Windows job runs on every pull request into the default branch and nightly, and **Phase 0 is not complete until it reports green**. A Windows job that cannot acquire a runner fails; it does not pass by absence.

Phase 0 records its decisions in the repository's documentation system with a clean division: ADRs own the reasons, persistent design documents own the current discipline, and the change package owns the generation context. The same content is not copied into all three.

## Alternatives considered

**A documents-only Phase 0 with verification deferred to Phase 1.** A narrower and defensible reading of "Phase 0 is contracts, not code". Rejected because it makes two of the four exit criteria unfalsifiable and silently moves them into a phase that has its own exit criteria to meet.

**Pulling the Phase 1 proof of concept forward.** Would produce real WASAPI capture and real detection in Phase 0. Rejected because it commits to platform behaviour before any measurement exists, which is what the spikes are for.

**A single continuous-integration tier that runs everything on Windows.** Simpler configuration. Rejected because it makes the development inner loop depend on a Windows host and hides accidental platform coupling in the core crates until a much later phase.

**Advisory tier registration.** Rejected because an advisory rule is one a test can be added without following, which is the exact false-green this regime exists to prevent.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Each Phase 0 exit criterion has a check that fails when the criterion is violated.
- A test that needs Windows cannot exist unregistered and therefore cannot silently never run.
- The contract-core crates are provably free of platform coupling from day one, which the later macOS phase inherits.
- Schema and type drift is impossible rather than merely discouraged.

**Negative.**

- Phase 0 grows a test suite, a continuous-integration pipeline and a hard dependency on a Windows 11 runner, without which the phase cannot be completed.
- Synthetic seams exist only to make Phase 0 checkable and must be maintained until the real implementations land.
- Adding a T2 test now requires a tier-file edit, which is friction on every future durability test.

**Neutral.**

- The tier file becomes a second inventory of verifications alongside the plan, kept honest by the registration check.
- ADRs remain at proposed until a decision maker accepts them; acceptance is a gate before the implementing units start.

## Confirmation

cargo xtask verify --tier portable (T0) on a non-Windows host, --check-registration (T0), --tier windows (T2); cargo test -p xtask ci_defines_portable_and_windows_gates (T0); dev-docs conformance --root docs (T0).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
