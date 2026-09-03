---
id: adr-20260903-detector-signal-replay-contract
kind: adr
title: Signals are UI-text-free facts and the detector is a pure replayable function
summary: A closed signal envelope with no free-text field, a lint-enforced pure detector
  citing evidence, a closed four-way outcome partition, and JSONL replay fixtures.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - The privacy constraint is enforced by the schema rather than by discipline.
  - Detection regressions are reproducible from committed fixtures without a machine
    in a particular state.
  - Every automatic decision can be explained from its recorded evidence alone.
  negative:
  - Adapters have less information to work with, so some meeting applications will
    be harder to detect and will fall back to manual.
  - Every new signal kind is a schema change with a version bump and a fixture upgrade,
    which is friction on a path Phase 1 will walk often.
  - JSONL fixtures for a large Phase 5 matrix will be large and slow to scan without
    a rebuilt index.
  neutral:
  - Purity means all clock and filesystem access happens in collectors, which shifts
    complexity rather than removing it.
  - The four-way partition forces every adapter to state what it does not know, which
    is more work than a boolean.
confirmation: cargo test -p ma-signal schema_contains_no_free_text_subject (T0); cargo
  test -p ma-detect replay_is_byte_identical (T1), every_decision_cites_evidence (T1),
  outcome_partition_is_total (T1); cargo xtask boundary --check forbidden-imports
  (T0).
tags:
- detection
- determinism
- privacy
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

PLAN section 3 forbids detection from depending on DOM structure, selectors, control labels, screen coordinates, accessibility paths or full URLs. PLAN section 5 requires that diagnostics explain the signals used for each decision, and PLAN section 9 requires a regression matrix over recorded scenarios that will still replay years from now.

A prohibition expressed as a rule gets violated by the first person who needs a window title. A prohibition expressed as a schema with nowhere to put one does not.

## Decision

A signal is a closed record whose `kind` and `subject` are closed enumerations and unions, with **no free-text field anywhere**: no window title, no control label, no coordinate, no accessibility path, no full URL. A DOM-derived fact has nowhere to live, which makes PLAN section 3's prohibition structural.

The detector is a **pure function** of the signal timeline plus the adapter table. Purity is enforced by the boundary check's forbidden-import list (`std::time`, `std::fs`, `std::net`, `std::process`, `rand` are unavailable to it) rather than by review. Replaying a timeline produces a byte-identical decision sequence across runs and processes, and **every decision cites the signal identifiers and the rule identifier it used**, which is what makes diagnostics explanatory.

The outcome space is a closed four-way partition — determinate, unknown, inconclusive, conflicting — with the default that the absence of a determinate outcome never starts capture. Unknown falls back to manual control; conflicting activates at most one session and records the losers as suppressed candidates.

Ordering is by monotonic time per source, merged across sources; wall-clock time is recorded for display and correlation only and never for ordering, so a clock step cannot reorder a timeline. A collector that starts while a condition is already true marks the signal as a restart resynchronization, and such a signal may raise a candidate but may never produce a determinate start, because the user was not present at the true beginning.

Replay fixtures are **JSONL** — a header record followed by one signal per line — with labels in a sidecar file keyed by time range. The header carries `schema_version` and `adapter_table_version` so that a later envelope change is survivable: a recorded timeline must stay replayable either through a tested upgrade function or by keeping the pinned old decoder, never by silently dropping fields. Phase 0 fixes the header and the rule but writes no upgrade function, because Phase 0 records no timelines; the obligation attaches to Phase 1, which owns both the corpus and the first envelope revision.

## Alternatives considered

**UI Automation and accessibility-tree probing for detection.** Far more informative and how many tools do it. Rejected by PLAN section 3, and structurally excluded here rather than merely forbidden.

**A free-text subject field with a redaction rule.** Would allow richer adapters. Rejected because redaction is a runtime behaviour that can be forgotten, whereas a schema without the field cannot.

**A detector that reads the clock and the filesystem directly.** Simpler to write. Rejected because it makes replay non-deterministic and the Phase 5 regression matrix worthless.

**An embedded SQLite fixture format.** Indexes better for a large matrix and can hold labels in one file. Rejected because these fixtures are reviewed in pull requests and appended to during live capture, both of which need a line-oriented text file; an index can be rebuilt from JSONL at analysis time, whereas a binary fixture that has become the truth can never be diffed again.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- The privacy constraint is enforced by the schema rather than by discipline.
- Detection regressions are reproducible from committed fixtures without a machine in a particular state.
- Every automatic decision can be explained from its recorded evidence alone.

**Negative.**

- Adapters have less information to work with, so some meeting applications will be harder to detect and will fall back to manual.
- Every new signal kind is a schema change with a version bump and a fixture upgrade, which is friction on a path Phase 1 will walk often.
- JSONL fixtures for a large Phase 5 matrix will be large and slow to scan without a rebuilt index.

**Neutral.**

- Purity means all clock and filesystem access happens in collectors, which shifts complexity rather than removing it.
- The four-way partition forces every adapter to state what it does not know, which is more work than a boolean.

## Confirmation

cargo test -p ma-signal schema_contains_no_free_text_subject (T0); cargo test -p ma-detect replay_is_byte_identical (T1), every_decision_cites_evidence (T1), outcome_partition_is_total (T1); cargo xtask boundary --check forbidden-imports (T0).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
