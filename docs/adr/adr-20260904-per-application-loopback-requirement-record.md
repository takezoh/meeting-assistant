---
id: adr-20260904-per-application-loopback-requirement-record
kind: adr
title: The process-tree loopback requirement stays in the measured comparison record,
  not in the adapter table
summary: Whether an application needs process-tree loopback is recorded only in the
  Windows-tier measured comparison record, whose declared required observations force
  one entry per application; no adapter.toml or AdapterSpec field is added in Phase
  1.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - The fact exists in exactly one place, the record that measures it, so no second
    copy can disagree with the measurement or go stale against it.
  - The L1 adapter contract that four L4 crates, the shared conformance suite and
    the composition root read is not widened for a value nothing in Phase 1 reads.
  - Nothing can force an adapter table version bump, so every committed decision identifier
    and the Phase 0 decisions golden stay valid and stay able to catch a real matching
    change.
  negative:
  - The fact lives in a manual record rather than in code, so no compiler or parser
    sees it and only the declared required observations keep it complete.
  - A later phase that wants the value at runtime must add the adapter field then,
    which is a second change to the same fact rather than one change now.
  - Reading the requirement means opening a record under the change package rather
    than a table beside the other per-service facts.
  neutral:
  - The measurement itself is unchanged and still owned by the capture contract; only
    the place the derived value is written moves.
  - The manual-verification procedure gains a required-observations declaration, which
    is a Phase 1 policy file the same family already introduces.
confirmation: cargo test -p xtask loopback_requirement_record_covers_every_adapter_table
  (T0); cargo xtask manual-record --id v-win1-loopback-requirement-live-comparison
  --require pass (T2, windows tier).
tags:
- adapters
- capture
- windows
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
source_paths:
- manual-verification.toml
- verification-tiers.toml
- xtask/src/verify.rs
- crates/ma-signal/src/adapter.rs
updated: '2026-09-05'
---

## Context

PLAN section 6 Phase 1 asks for a "per-application record of whether process-tree loopback is required".
`CaptureMode` records the mode a given *track* actually used, but nothing in the repository holds the
application-level fact, and no evidence names a home for it.

The design draft proposed an additive `adapter.toml` field as a documented candidate and left the decision
open, with the alternative baseline of a new `ma-store` table keyed by application. The design critique raised
two further problems that a location decision alone does not solve: nothing derived the value (both the capture
unit and the record unit disclaimed producing it, so a hand-authored guess would still pass a parser-only
test), and nothing said what happens to `adapter_table_version`, which every committed fixture header and every
`DetectorOutput` carries and which `Decision::derive` mixes into every decision identifier.

The first problem was closed by assigning the measurement: `contract-process-loopback-capture` owns a
Windows-tier procedure that captures the same meeting twice against the same application, once under
single-process activation and once under process-tree activation, and records per application whether the
second captured audio the first missed. That closed the second problem too, but in a way that made the adapter
field redundant rather than necessary — which is what the minimality audit then found.

## Decision

The per-application process-tree-loopback requirement is recorded **only** in the Windows-tier measured
comparison record that `v-win1-loopback-requirement-live-comparison` commits. Phase 1 adds no field to
`adapter.toml`, to `AdapterSpec` or to any other shared contract, and does not touch `adapter_table_version`.

The record is kept complete rather than merely present. `manual-verification.toml`'s procedure for that
identifier declares the observation keys its record must carry — one per adapter table, read from the tables
discovered under `crates/ma-adapter-*/adapter.toml` rather than written as literals, because `boundary.toml`
confines service identifiers to L4 crates and `xtask` is L5. `cargo xtask manual-record --id <id> --require
pass` rejects a record that omits a declared key, so a record cannot claim `pass` while naming three of the
four applications, and `loopback_requirement_record_covers_every_adapter_table` is the portable check on both
the declaration's completeness and the gate's rejection.

Adding the adapter field is **deferred**, not refused, and the condition is named: the phase that gives the
value a behavioural consumer — a capture path that selects the activation mode from a declared value instead of
probing at runtime, or a match rule that reads it. That phase decides the version-bump question on the evidence
it then has.

## Alternatives considered

**An additive `requires_process_tree_loopback` field on each `adapter.toml`, parsed into `AdapterSpec`.** The
design draft's candidate, and this plan's own position until the minimality audit. Rejected because Phase 1
gives it no behavioural consumer: no match rule reads it, `adapter_table_version` is deliberately not bumped
for it, and the only assertion available to an adapter conformance test is that the field equals the value the
comparison record already states. That makes it a second machine-readable copy of a recorded observation,
placed inside an L1 contract that four L4 crates, the shared `conformance_violations()` suite and the
composition root all read, buying no observable outcome the record does not already produce. A copy that no
code consults is a copy that can silently disagree with its source.

**A new `ma-store` table keyed by application.** Correct if the fact were a per-recording observation. Rejected
because it is policy data read before any recording exists: it would need a writer-role assignment under
`contract-store-ownership`, a schema migration, and a read path from a layer that does not otherwise touch the
store, all so that a boolean can be looked up.

**A separate Phase 1 policy file at the repository root.** Consistent with `boundary.toml` and
`egress-inventory.toml`, and it would leave the shared adapter contract untouched. Rejected because the
measured comparison record is already such a file, already committed, already gated, and already the artefact
the value is derived from; a second one would split the fact from its evidence.

**Bump `adapter_table_version` and regenerate the goldens.** The conservative reading of a schema change, and
the design critique's own patch hint. Rejected because `Decision::derive` mixes the table version into the
material that derives every decision identifier, so a bump would rewrite every identifier in Phase 0's
committed golden and in the five Phase 0 fixture headers for a field that changes no decision — and because
after this decision there is no field to bump for.

## Consequences

**Positive.**

- One home for the fact, and it is the artefact that measures it, so no copy can drift from the measurement.
- The shared adapter contract is not widened for a value nothing in this phase reads.
- No committed decision identifier, fixture header or table version moves, and `replay_is_byte_identical` stays
  a check for a real matching change rather than for bookkeeping.

**Negative.**

- The fact lives in a JSON record rather than in typed code; no parser validates its meaning, and only the
  declared required observations keep it from being partial.
- A later phase that needs the value at runtime pays for a second change to the same fact.
- Reading the requirement means opening a record under the change package rather than a table beside the other
  per-service facts.

**Neutral.**

- The measurement is unchanged and still owned by `contract-process-loopback-capture`; only where the derived
  value is written moves.
- The procedure gains a required-observations declaration, inside the manual-verification policy file the same
  family already introduces.

## Confirmation

`cargo test -p xtask loopback_requirement_record_covers_every_adapter_table` (T0), which fails when the
procedure declares fewer required observations than there are adapter tables or when the gate accepts a record
that omits one; `cargo xtask manual-record --id v-win1-loopback-requirement-live-comparison --require pass`
(T2, windows tier), which fails when the record is absent, not `pass`, incomplete, or stale against its
procedure. `crates/ma-signal/src/adapter.rs` appears in `source_paths` as the contract this decision keeps
unchanged; a diff that adds a loopback field there contradicts this record.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
