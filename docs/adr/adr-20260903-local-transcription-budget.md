---
id: adr-20260903-local-transcription-budget
kind: adr
title: Local transcription budget of at most real time, with overrun as a warning
summary: A two-hour recording must transcribe locally within two hours on CPU, with
  mandatory progress and cancellation, per-item cost convergence, and overrun treated
  as a warning rather than a failure.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - A degrading implementation fails a test rather than a user's afternoon.
  - The user can always see progress and always stop the work.
  - A slow machine still produces a transcript.
  negative:
  - The convergence test requires a 240-item run, which makes the processor suite
    slow.
  - The reference CPU class is a moving target that has to be pinned by measurement
    and revisited.
  - Treating overrun as a warning means the product can ship an experience that misses
    its own budget without failing anything.
  neutral:
  - The default model and quantisation are expected to change once measured; the contract
    survives either outcome.
  - The five-second cancellation bound is enforceable because native work runs in
    a killable child process.
  - The 30-second per-item budget and the 150-second stall timeout are product defaults
    chosen so that "no progress" is falsifiable; changing either is a settings and
    test change, not a design change.
confirmation: cargo test -p ma-processor progress_is_monotonic (T1), cancellation_observed_within_bound
  (T1), per_item_cost_does_not_grow (T1), budget_overrun_emits_warning_not_failure
  (T1).
tags:
- performance
- processing
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes the budget: without a GPU, a two-hour recording must finish local transcription within two hours — at most one times real time — with progress display and cancellation mandatory, and exceeding the budget treated as a warning rather than a failure.

A budget stated as a total is satisfiable by an implementation that is fast at the start and quadratic later, and such an implementation passes a short test and fails a real meeting. The budget therefore needs a shape, not just a ceiling.

## Decision

Local transcription of a two-hour recording completes within two hours of wall-clock time on the reference CPU class. Progress is reported at least once per work item and never decreases. Cancellation is observed within one work item and **within five seconds**, measured as an elapsed interval rather than as a flag being set.

A processor that stops reporting is not treated as healthy. The per-item budget is the work item's own media duration — **30 seconds** for the chunk-sized items the cancellation rule already forces — and a step that emits no progress frame for **150 seconds**, five times that budget, is stalled: the supervisor kills the host child and the step becomes retryable with its completed work items preserved. A stall is deliberately classified apart from a host crash, because a stall is observed while a crash is inferred from an exit status, and the two want different retry and diagnostic treatment.

The budget additionally requires **cost convergence**: per-work-item cost must not grow across a long run. A summarization or transcription processor that accumulates prior context into each request turns a 240-item job quadratic and would satisfy a total-time budget on a short fixture while failing a real meeting; the convergence check over a 240-item run catches it.

Exceeding the budget emits a warning and the step still succeeds. The budget is a promise about the experience, not a correctness condition, and failing a completed transcription because it was slow would destroy work the user wanted.

## Alternatives considered

**Overrun falls back to the external API.** Would keep the wall-clock promise. Rejected because it transmits meeting audio to a third party as a side effect of slowness, which violates the explicit-transmission consent PLAN requires.

**Overrun fails the step.** Rejected because it throws away a transcription that was going to finish, in the name of a performance target.

**A total-time budget with no per-item shape.** Rejected because it is passed by implementations that degrade with length, which is the failure that matters.

**No cancellation requirement.** Rejected because a multi-hour job the user cannot stop is worse than one that is slow.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- A degrading implementation fails a test rather than a user's afternoon.
- The user can always see progress and always stop the work.
- A slow machine still produces a transcript.

**Negative.**

- The convergence test requires a 240-item run, which makes the processor suite slow.
- The reference CPU class is a moving target that has to be pinned by measurement and revisited.
- Treating overrun as a warning means the product can ship an experience that misses its own budget without failing anything.

**Neutral.**

- The default model and quantisation are expected to change once measured; the contract survives either outcome.
- The five-second cancellation bound is enforceable because native work runs in a killable child process.
- The 30-second per-item budget and the 150-second stall timeout are product defaults chosen so that "no progress" is falsifiable; changing either is a settings and test change, not a design change.

## Confirmation

cargo test -p ma-processor progress_is_monotonic (T1), cancellation_observed_within_bound (T1), per_item_cost_does_not_grow (T1), budget_overrun_emits_warning_not_failure (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
