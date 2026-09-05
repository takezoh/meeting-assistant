---
id: adr-20260904-verification-registry-multi-plan-and-manual-records
kind: adr
title: The registry holds many plans, and a human observation is a digest-pinned record
summary: verification-tiers.toml declares a list of canonical plans whose verification
  ids are unioned, and an observation a hosted runner cannot make is registered as
  a declared procedure plus a committed record gated by cargo xtask manual-record.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - Adding a phase's plan stops invalidating the previous phase's registrations, so
    the registry scales past one plan without any registration losing its meaning.
  - An observation only a person on real hardware can make becomes a gate the hosted
    windows job can actually run, instead of a command that fails for want of an application
    or passes vacuously.
  - Editing a procedure invalidates every record taken against the old text, so a
    procedure change cannot silently inherit an old result.
  negative:
  - Phase 1 completion depends on someone having a Windows machine with all four target
    applications installed, and the nightly windows job stays red until nine records
    exist.
  - A record asserts that a person performed a procedure and what they saw; its truthfulness
    is not machine-checkable, only its presence, outcome and freshness are.
  - Two more repository policy files now have to stay consistent with the plans, which
    is more surface for drift than one.
  neutral:
  - The tier definitions, the exactly-once rule, the plan-tier-to-tier mapping and
    the CI job list are unchanged; only the plan source and one subcommand are added.
  - The single plan field keeps working as a one-element form, so no other consumer
    of verification-tiers.toml breaks.
confirmation: cargo test -p xtask registration_unions_every_declared_plan (T0), every_manual_verification_id_has_a_procedure
  (T0), a_record_whose_procedure_changed_is_rejected (T0); cargo xtask verify --check-registration
  (T0).
tags:
- verification
- ci
- windows
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
- {type: references, target: adr-20260903-phase0-executable-contract-skeleton}
source_paths:
- verification-tiers.toml
- xtask/src/verify.rs
- .github/workflows/ci.yml
updated: '2026-09-05'
---

## Context

Phase 0 built the registry: `verification-tiers.toml` names one canonical plan, and
`cargo xtask verify --check-registration` requires every id that plan declares to be registered exactly once,
in the tier its plan tier implies, with `platform = "windows"` for the windows tier. `Registration.command` is
a required, non-empty `String`, and an unregistered T2 is a build failure — the rule that makes an unrun
Windows check fail rather than silently pass.

Phase 1 breaks two of its assumptions.

**One plan.** `check_registration` reads a single `file.plan` path and reports "registered but not declared by
the plan (stale registration)" for every registration outside it. Phase 1 is the repository's second plan.
Repointing the field would make all 112 Phase 0 registrations stale, including the ids that
`docs/design/*.md` invariants cite and that the `design-set` rule of `cargo xtask docs-check` requires to stay
registered.

**Every check is a command.** The windows job runs on a hosted `windows-latest` image with no Teams, Slack,
Zoom or Chrome installation, no speaker, no microphone, on a nightly and pull-request cadence. Nine Phase 1
verifications need exactly those, or two hours of wall clock, or a person judging a comparison between two
recordings. The Phase 1 draft registered six of them as bare unattended commands and left two with
`command: null`, which the registry cannot even deserialise. `planning-source.txt` states the requirement
directly: checks needing real Windows hardware or real applications are split between the CI windows job and a
manual procedure.

## Decision

**A list of plans.** `verification-tiers.toml` gains `plans = [...]`, and `check_registration` takes the union
of the verification ids those plans declare as the registered set. The existing single `plan` field keeps
working as a one-element form, so no other consumer breaks. Every other rule is unchanged: exactly once,
plan-tier-to-tier mapping, platform binding, and a non-empty command.

**A manual observation is a declared procedure plus a record.** `manual-verification.toml` at the repository
root — the same pattern as `boundary.toml`, `verification-tiers.toml` and `egress-inventory.toml`, a policy
file with a conformance check — declares per manual verification id: the owner, the host profile, the ordered
steps, the artifact path, the pass criterion, and the digest of the procedure text. A performed observation is
a committed JSON record under the change package naming the id, when and by whom it was performed, the host
profile, the outcome (`pass`, `fail` or `blocked`), the observations, and the procedure digest it was performed
against.

The **registered command** is `cargo xtask manual-record --id <id> --require pass`, which the hosted runner can
run even though it cannot make the observation. It fails when the record is absent, when the outcome is not
`pass`, or when the recorded digest differs from the current procedure text — so editing a procedure
invalidates every record taken against the old one.

These ids stay in the **windows tier at plan tier T2**, because that job is the phase exit gate and that is
where PLAN puts the obligation.

## Alternatives considered

**Repoint the single `plan` field at Phase 1's spine and carry Phase 0's ids forward.** The critique's patch
hint. Rejected because carrying them forward means either leaving 112 registrations stale or copying Phase 0's
verification ids into Phase 1's spine, which would have Phase 1's plan declare contracts it does not own; and
because the `design-set` docs rule fails when a design invariant cites an id that is no longer registered.

**A third `manual` tier in `verification-tiers.toml`.** Semantically tidier. Rejected because
`check_registration` maps plan tier `T2` to tier `windows` and everything else to `portable`, so a third tier
needs that mapping to grow a case, the `--tier` runner to grow a mode, and the CI workflow to grow a step —
for no guarantee that registering the record check in the existing windows tier does not already give.

**Leave the six checks as bare unattended commands.** Zero new machinery. Rejected because each would fail on
the hosted runner for want of an installed application, and a `-- --ignored` variant would pass by running
nothing, which is the exact "written but never run" failure `contract-verification-tiering` exists to prevent.

**Drop the nine checks.** Smallest. Rejected because they are PLAN section 6 Phase 1 exit criteria: the
two-hour recording, the per-application echo documentation, the contamination documentation and the live Meet
detection are the phase.

**A plain Markdown procedure document with no checker.** Cheapest to write. Rejected because an unchecked
document passes silently, which is the same failure mode as an unrun test; the digest pin is what makes a stale
record fail.

## Consequences

**Positive.**

- Adding a phase's plan stops invalidating the previous phase's registrations.
- An observation only a person on real hardware can make becomes a gate the hosted windows job can run.
- A procedure change cannot silently inherit an old result.

**Negative.**

- Phase 1 completion now depends on hardware access: someone needs a Windows 11 machine with Teams, Slack, Zoom
  and Chrome installed, and the nightly windows job stays red until nine records exist. That is honest about
  what the exit criteria require, and it is a real scheduling constraint.
- A record asserts that a person performed a procedure and what they observed. Only its presence, outcome and
  freshness are machine-checkable; its truthfulness is not.
- Two repository policy files now have to stay consistent with the plans instead of one.

**Neutral.**

- Tier definitions, the exactly-once rule, the plan-tier mapping and the CI job list are unchanged.
- The single `plan` field keeps working, so nothing else that reads the registry breaks.

## Confirmation

`cargo test -p xtask registration_unions_every_declared_plan` (T0);
`every_manual_verification_id_has_a_procedure` (T0), which fails on a manual id with no procedure and on a
procedure naming no plan-declared id; `a_record_whose_procedure_changed_is_rejected` (T0);
`cargo xtask verify --check-registration` (T0), already registered as `v-tier-every-t2-registered`.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
