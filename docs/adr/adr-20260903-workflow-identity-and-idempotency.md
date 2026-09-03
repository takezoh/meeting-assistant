---
id: adr-20260903-workflow-identity-and-idempotency
kind: adr
title: Time-ordered identifiers, an intent-before-effect ledger, and generation plus
  overlay for user edits
summary: UUIDv7 identifiers reproduced verbatim across surfaces, an effect ledger
  that commits intent before any external effect, and immutable generations with a
  separate edit overlay.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - One identifier string locates the same entity in the database, on disk and in
    an export payload.
  - The duplicate-export failure mode is closed by a procedure rather than by an assumption
    about commit ordering.
  - A user edit is always either re-applied or visibly orphaned, and never silently
    gone.
  - Regenerating with a different model keeps the previous result rather than replacing
    it.
  negative:
  - The effect ledger is an extra durable write before every external effect, which
    costs a transaction on every step and export.
  - Immutable generations plus overlays mean the stored data grows with every regeneration
    and rendering requires a composition step.
  - An unresolvable unknown outcome surfaces a decision to the user, which is a user-visible
    consequence of a crash.
  - UUIDv7 path segments make the artifact tree unreadable by hand.
  neutral:
  - The re-anchoring rule for text edits is deliberately left to Phase 3; only the
    invariant that decides whether Phase 3 got it right is fixed here.
  - Linked sessions require the library to present a continuation as one meeting.
confirmation: cargo test -p ma-workflow duplicate_enqueue_is_noop (T1), lease_recovery_creates_no_duplicate_artifact
  (T1), regeneration_preserves_user_edits (T1); cargo test -p ma-destination crash_before_identity_record_reconciles
  (T1); cargo test -p ma-engine --test durability recovery_reuses_session_id (T2).
tags:
- data-model
- idempotency
- workflow
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

"Workflow steps and artifacts have stable identifiers and states" is a Phase 0 exit criterion, and PLAN section 7 requires that a retried export never create a duplicate remote object and that user edits survive regeneration. The Drive `drive.file` scope cannot discover objects the application did not create, so a lost identity cannot be recovered by searching by name.

The hardest cases are the crash windows: between creating a remote object and recording its identifier, and between a processor succeeding and its completion being written down.

## Decision

Every entity carries a UUIDv7 identifier — time-ordered, sortable, collision-free without coordination — assigned by the component that owns the entity, persisted before any side effect that references it, and reproduced **verbatim** in database rows, filesystem path segments and export payloads, so one string finds the others. Recovery reuses the identifier found on disk and never mints a replacement.

Step identity is a hash over session, step kind, ordered input artifacts, processor identifier, processor version and configuration digest. Enqueueing a key already recorded as succeeded returns the recorded result and executes nothing. Changing a processor, its version or its configuration produces a different key and a new step, and the previous result is retained, which is what makes regeneration non-destructive.

**Intent before effect.** "Idempotent or committed in the same transaction" is an intention that an implementation can satisfy by committing after a remote create, which is the window that produces duplicates. The procedure is therefore fixed: commit an `effect_ledger` row as `intended` before any effect outside the state database, apply the effect, then update the row to `applied` with the resource reference. After a restart, an `intended` with no `applied` is the named outcome **unknown**, resolved by the owning contract's lookup path or by an explicit user decision — never by a silent recreate.

**User edits** are never stored over generated content. Each processor run appends an immutable `generation` row; user edits live in a separate `edit_overlay` layer and what a user sees is the latest generation composed with the overlay at read time. Regeneration adds a generation and does not touch the overlay. An overlay that cannot be re-anchored is kept with an orphaned flag and listed as an edit that could not be re-applied, never deleted. Speaker-label edits anchor to the **speaker cluster** rather than to a transcript segment, because a different model re-segments the transcript but usually preserves the cluster. An edit offered with no anchor basis is refused rather than stored, because an overlay with no anchor would become a silent loss at the next regeneration.

An interrupted recording **finalizes and links** through `continues_from` rather than resuming into the same session.

## Alternatives considered

**Auto-increment integer identifiers.** Compact and natural in SQLite. Rejected because they require the database to be the assigner, which breaks when the directory is the truth and the row is written afterwards.

**Random UUIDv4.** Rejected because it is not time-ordered, so every index on it fragments and time-range queries need a separate column.

**Discovering remote objects by name search before creating.** Rejected because the `drive.file` scope cannot see objects the application did not create, and because names are not unique.

**Storing edits by overwriting the generated text.** Simplest to render. Rejected because regeneration then has to diff its way back to the user's intent, and the failure mode is silent loss.

**Resuming into the same session after an interruption.** Gives the user one meeting instead of two. Rejected because it reopens a finalized track and re-runs consolidation for a library presentation concern that linking already solves.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- One identifier string locates the same entity in the database, on disk and in an export payload.
- The duplicate-export failure mode is closed by a procedure rather than by an assumption about commit ordering.
- A user edit is always either re-applied or visibly orphaned, and never silently gone.
- Regenerating with a different model keeps the previous result rather than replacing it.

**Negative.**

- The effect ledger is an extra durable write before every external effect, which costs a transaction on every step and export.
- Immutable generations plus overlays mean the stored data grows with every regeneration and rendering requires a composition step.
- An unresolvable unknown outcome surfaces a decision to the user, which is a user-visible consequence of a crash.
- UUIDv7 path segments make the artifact tree unreadable by hand.

**Neutral.**

- The re-anchoring rule for text edits is deliberately left to Phase 3; only the invariant that decides whether Phase 3 got it right is fixed here.
- Linked sessions require the library to present a continuation as one meeting.

## Confirmation

cargo test -p ma-workflow duplicate_enqueue_is_noop (T1), lease_recovery_creates_no_duplicate_artifact (T1), regeneration_preserves_user_edits (T1); cargo test -p ma-destination crash_before_identity_record_reconciles (T1); cargo test -p ma-engine --test durability recovery_reuses_session_id (T2).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
