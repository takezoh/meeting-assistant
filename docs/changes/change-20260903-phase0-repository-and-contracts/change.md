---
id: change-20260903-phase0-repository-and-contracts
kind: change
title: 'Phase 0: repository structure, contracts, and Phase 0 decisions'
summary: Establish the cargo workspace, module boundary enforcement, session/signal/artifact/processor/destination
  contracts, threat model and verification tiering for the Windows 11 MVP, and record
  fifteen Phase 0 ADRs.
status: ready
created: '2026-09-03'
profile: sdd@1
intent: Turn PLAN.md's Phase 0 deliverable list into falsifiable, mechanically checked
  contracts that bind Phase 1 to 5, and close the seven decisions PLAN section 8 defers
  to before or during Phase 0, so that each Phase 0 exit criterion has a check that
  fails when the criterion is violated rather than a document that asserts it does
  not.
outcomes:
- A cargo workspace with declared layers whose dependency-direction, forbidden-literal
  and processing-isolation rules are enforced by cargo xtask boundary and proven non-vacuous
  by a negative fixture carrying both planted violations and planted decoys.
- A meeting-session state model, automatic-recording mode policy and consent-surface
  rule in which every timing bound is a fixed number and no audio byte reaches the
  artifact root before the recording state.
- Signal and detector contracts in which a UI-derived fact has nowhere to live, replay
  is byte-identical and every decision cites the evidence it used.
- A recording and artifact model with a stated 30-second loss bound, honest gap-preserving
  recovery, verify-before-delete consolidation, relocatable artifact addressing, and
  a deletion path that converges to a content-free tombstone.
- Processor and destination contracts with capability declaration, staged inputs,
  argument-vector invocation, per-job child-process execution, an intent-before-effect
  ledger and a local egress audit.
- A threat model and credential policy in which secrets exist in exactly one place,
  meeting content cannot be logged by type, and every reachable outbound host is declared
  in a build-checked repository egress inventory.
- A two-tier verification regime in which the contract-core crates build on a non-Windows
  host and every T2 verification is registered and run on a Windows runner before
  Phase 0 is called complete.
- Fifteen ADRs recording every Phase 0 decision, including all seven questions PLAN
  section 8 asks to be resolved before or during Phase 0, each with named decision
  makers and a non-empty negative consequence list.
scope:
- Cargo workspace, crate topology, composition roots, toolchain pin and CI entry.
- boundary.toml, the dependency-direction and two-class literal scan, the capture-path
  and native-linkage isolation rules, and their negative fixture.
- 'Meeting-session state model: states, transitions, causes, automatic-recording modes,
  countdown and hysteresis timing, consent surfaces, crash recovery states.'
- 'Signal contract and detector contract: envelope, ordering, JSONL replay fixtures,
  purity, evidence-carrying decisions, outcome partition, adapter isolation.'
- 'Recording and artifact model: track descriptors, chunking, durability ordering,
  gap-preserving timeline, consolidation, artifact addressing, identifier scheme,
  deletion and purge.'
- Engine and interface process topology, the JSON-RPC-over-named-pipe control channel,
  transport authorization and the build-channel carve-out.
- 'Browser-extension loopback channel contract: message schema, authentication, freshness,
  corroboration.'
- 'Local store contract: SQLite table families, two-writer ownership, migration and
  schema-version rules.'
- 'Workflow contract: step identity, effect ledger, retry classes, artifact lifecycle,
  generation and edit-overlay model.'
- Processor and destination contracts including the processor host child process and
  export identity.
- Threat model, credential policy, diagnostic redaction, egress inventory and signed-manifest
  trust.
- 'Verification tiering: verification-tiers.toml, tier registration and the two CI
  gates.'
- 'Documentation materialization: fifteen ADRs, the persistent design documents this
  phase introduces, and the three change members.'
non_goals:
- Real Windows signal collection and real WASAPI capture — Phase 1; Phase 0 ships
  the seams and a synthetic source behind them.
- Detection heuristics, thresholds and the per-application validation matrix — Phase
  1 and Phase 5.
- Any transcription, diarization or summarization implementation — Phase 3; Phase
  0 fixes the contract and the budget.
- Real Google Drive and Notion clients — Phase 4; Phase 0 fixes the destination contract
  and export identity.
- The browser extension itself — Phase 1; Phase 0 fixes the channel contract it must
  satisfy.
- Default retention values — PLAN section 8 scopes them to before Phase 2; Phase 0
  fixes only the deletion mechanism and leaves the grace period without a default.
- macOS, video capture, real-time translation and participant bots — PLAN section
  4 non-goals.
- Accepting the ADRs this change creates; acceptance is an authority act reserved
  to the decision makers.
change_classes:
- behavior
- responsibility
- boundary
- dependency
- invariant
- capability
governance:
  gate: hard
  approval_evidence: 'consultation-phase0-20260903-1 (2026-09-03): the user accepted
    all fifteen Phase 0 ADRs via the design skill consultation; each ADR journal records
    the proposed -> accepted transition.'
  reasons:
  - Establishes the trust boundaries, credential custody, control-channel authorization
    and egress policy for a product that records meetings; a wrong decision here is
    expensive to reverse and affects every later phase.
  - Fixes the process topology, store writer ownership and module boundaries that
    bind Phase 1 to 5; these are non-reversible without reworking the crate graph.
  - Requires acceptance of fifteen proposed ADRs by the named decision makers before
    implementation starts; approval_evidence must record that acceptance before this
    change becomes active.
members:
- role: requirements
  path: changes/change-20260903-phase0-repository-and-contracts/requirements.md
  required: true
- role: implementation
  path: changes/change-20260903-phase0-repository-and-contracts/implementation.md
  required: true
- role: verification
  path: changes/change-20260903-phase0-repository-and-contracts/verification.md
  required: true
promotion:
- target: none
  section: none
  action: none
  item: {}
  reason: A promotion entry upserts or retires a stable item of an existing design
    document, and this change introduces the repository's first persistent design
    documents rather than amending any. docs/design/ is empty, so there is no indexed
    target to upsert into at manifest time; the unit docs-and-adr-materialization
    authors five documents — docs/design/module-boundaries.md, session-lifecycle.md,
    recording-artifact-model.md, threat-model.md and credential-policy.md (trust boundaries
    are a section of the threat model, not a sixth document) — and the responsibility,
    boundary, invariant and capability content that would otherwise be promoted is
    written directly into them. Every subsequent change that alters one of those items
    promotes into the owning document normally.
unresolved_decisions: []
tags:
- phase-0
- architecture
- boundaries
- contracts
- security
- windows
owners:
- take
relations: []
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Summary

Phase 0 produces structure, contracts and decisions — not meeting functionality. Its test of success is not
that the documents exist; it is that each Phase 0 exit criterion has a check that fails when the criterion is
violated. The four criteria translate as follows: "core boundaries do not require a proprietary backend"
becomes a build-checked egress inventory with no first-party owner value available to write; "meeting-service
logic cannot leak into the workflow core" becomes a dependency-direction and literal check proven
non-vacuous by a fixture that carries decoys as well as violations; "the capture engine continues recording
after the UI terminates" becomes an integration test that kills the interface, and a harder one that aborts a
processor child; and "workflow steps and artifacts have stable identifiers and states" becomes time-ordered
identifiers reproduced verbatim across three surfaces plus an intent-before-effect ledger that names the
crash window instead of assuming it away.

The seven questions PLAN section 8 asks to be resolved before or during Phase 0 are all closed here: the
desktop framework and IPC mechanism, the database and artifact layout, the audio format, the definition of
automatic recording modes, the initial adapters, the local transcription time budget, and update and adapter
manifest distribution. Eight further choices the design drafts preserved are also closed — where the workflow
runtime lives, whether native processors are isolated, how many database writers exist, what happens after an
interrupted recording, how much executable skeleton Phase 0 ships, the extension transport, the fixture
format, and what counts as a consent surface — each with its alternatives and rejection reasons recorded in
an ADR.

Three of those closures changed a user-observable behaviour and are worth stating plainly. The consent
surface is now the engine's own operating-system notification rather than an attached client, so automatic
recording works with the window closed, which is the case the separate engine process exists for. Deletion
now has an owner, a two-phase mechanism and a content-free tombstone, so a deleted meeting is provably gone
from a design that otherwise splits truth between a directory and a database. And no audio byte reaches the
artifact root before the recording state, so "cancel before recording starts" is observable on disk rather
than only in the interface.

The canonical design plan, with the full contract set and its reasoning, is copied into `design-plan/` in
this package. `requirements.md` carries the EARS requirements and acceptance criteria, `implementation.md`
the contracts, seams and unit order, and `verification.md` the tiered checks and the gates that make an
unrun Windows suite a failure rather than a silent pass.

## Closure Notes


{% transition from="draft" to="ready" date="2026-09-03" %}
design skill run design-phase0-20260903-1: all 15 ADRs accepted, verify-final approved, docs lint --conformance ok
{% /transition %}
