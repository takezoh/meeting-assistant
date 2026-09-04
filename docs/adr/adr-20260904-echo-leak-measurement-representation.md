---
id: adr-20260904-echo-leak-measurement-representation
kind: adr
title: Echo leak is one echo-return-loss number over one qualifying window
summary: Meeting-audio leakage into the microphone track is measured as the difference
  of sixty-second RMS levels between the loopback and microphone tracks, with three
  outcomes, and is recorded outside the signal envelope.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - Two conforming implementations produce the same number for the same recording,
    so the per-application severity PLAN asks for is comparable across applications
    and across runs.
  - A sixty-second energy ratio is insensitive to the track alignment uncertainty
    that exists by construction, so the measurement does not silently depend on an
    alignment the recording model refuses to promise.
  - A recording that cannot be measured says so with a named outcome instead of reporting
    a number produced from unqualified audio.
  negative:
  - A single window per recording misses an application whose echo varies with volume
    or with the speaker in use, so the measurement characterises a condition rather
    than bounding a worst case.
  - The window admission conditions require a stretch with remote speech and no local
    speech, which a short or highly interactive meeting may never contain.
  - Echo return loss says nothing about how audible the leak is after downstream processing,
    so it ranks applications rather than predicting transcript damage.
  neutral:
  - The measurement is capture-side data with its own record file, so no signal, no
    Payload field and no store column changes.
  - The method is deliberately cheaper than a correlation or a lag search, which is
    a stated trade of resolution for reproducibility.
confirmation: cargo test -p ma-capture leak_erl_from_paired_fixture_tracks (T0) and
  leak_measurement_reports_no_qualifying_window (T0); cargo xtask manual-record --id
  v-win1-leak-live-per-app --require pass (T2).
tags:
- audio
- capture
- measurement
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
source_paths:
- PLAN.md
- crates/ma-core-types/src/timeline.rs
- crates/ma-signal/src/envelope.rs
updated: '2026-09-04'
---

## Context

PLAN section 6 Phase 1 asks for "measurement of meeting audio leaking into the microphone track when speakers
are used" and makes "echo conditions and their severity are documented per application" an exit criterion. No
schema, store table or ADR defines a methodology, a unit or a storage location.

The design draft proposed reusing `Payload.level_dbfs: Option<i16>` on an `AudioActivity` signal and left the
statistic, the window length, the unit semantics and the time base open, with the note that the representation
would be confirmed later. The design critique showed why that is not a deferral but a defect: two conforming
implementations would produce non-comparable per-application numbers that both pass the same fixture test, so
"severity is documented per application" would be unfalsifiable. It also noted that
`tracks_have_independent_origins` and `SessionTimeline.alignment_uncertainty_ms` exist precisely because sample
*n* of one track is not contemporaneous with sample *n* of another, so any frame-wise statistic over paired
tracks has an undefined alignment basis.

## Decision

**Statistic.** Echo return loss in dB:

    erl_db = rms_dbfs(loopback track over W) - rms_dbfs(microphone track over W)

A higher value means less leak. The unit is dB, a difference of levels, not dBFS, a level.

**Window `W`.** The first contiguous sixty-second window that satisfies both admission conditions:

- the loopback track's sixty-second RMS is at least −40 dBFS, so the meeting application really is producing
  audio through the speaker; and
- no twenty-millisecond frame of the microphone track exceeds −20 dBFS, so the local participant is not
  speaking over the measurement.

**Alignment basis.** `W` is located on each track by that track's own `TrackOrigin.start_monotonic_ns`. A
sixty-second energy comparison is insensitive to the tens of milliseconds `alignment_uncertainty_ms` can carry,
which is why an energy ratio is chosen over a frame-wise correlation or a cross-correlation lag search. The
recorded value carries the uncertainty so a reader can judge it.

**Outcomes.** Exactly three, and a missing measurement is always one of the last two rather than a zero:

- `measured` with `erl_db`, the window's start sample on each track, both RMS values and
  `alignment_uncertainty_ms`;
- `no_qualifying_window`, when no sixty-second window satisfies both admission conditions;
- `inconclusive_alignment`, when the session's alignment uncertainty exceeds one second.

**Storage.** A per-application measurement record produced by `ma-capture`, referenced by the Windows-tier
manual record for `v-win1-leak-live-per-app`. Not a signal, not a `Payload` field, not a store column.

## Alternatives considered

**Reuse `Payload.level_dbfs` on an `AudioActivity` signal.** The draft's default, and the field's type already
fits. Rejected on two grounds. Semantically, a derived cross-track statistic over a minute is not an
observation of one subject at one instant, which is what every other `Payload` field is; `level_dbfs` is read
by four crates, the JSON Schema under `contracts/` and the conformance suite, and overloading it makes every
consumer's reading of it depend on the signal's kind. Procedurally, `NFR-104` would demand an ADR-gated
`schema_version` bump for the reinterpretation anyway, so the reuse buys nothing over a dedicated field.

**A dedicated `Payload.leak_dbfs` field with a `schema_version` bump.** Honest about the semantics, but it
still puts a per-application, per-recording statistic into a per-signal envelope and obliges the fixture
upgrade that `adr-20260903-detector-signal-replay-contract` scopes to a real envelope revision. Rejected for
the same reason the endpoint left the envelope in
`adr-20260904-mic-endpoint-observed-outside-the-signal-envelope`: no detector rule reads it.

**A cross-correlation with a lag search.** Higher resolution, and it would recover the true acoustic delay.
Rejected because it makes the result depend on an alignment the recording model explicitly refuses to promise,
because it is far more expensive over a two-hour recording, and because Phase 1 needs to rank four applications,
not to build an echo canceller.

**Frame-wise median of per-frame level differences.** Robust to outliers, but a twenty-millisecond frame is
smaller than the alignment uncertainty the session timeline records, so the statistic would be corrupted by
exactly the misalignment the recording model warns about.

## Consequences

**Positive.**

- Two conforming implementations produce the same number for the same recording, so per-application severity is
  comparable across applications and across runs.
- The measurement does not silently depend on an alignment the recording model refuses to promise.
- A recording that cannot be measured says so with a named outcome rather than reporting a number computed from
  unqualified audio.

**Negative.**

- One window per recording characterises a condition rather than bounding a worst case; an application whose
  echo varies with output volume or with the speaker in use is under-described.
- The admission conditions require a stretch of remote speech with no local speech, which a short or highly
  interactive meeting may never contain, so `no_qualifying_window` will be a real outcome and not only a
  defensive branch.
- Echo return loss ranks applications; it does not predict how much a transcript will suffer.

**Neutral.**

- Nothing in the signal envelope, the store schema or the fixture format changes.
- The method is deliberately cheaper than a correlation, trading resolution for reproducibility.

## Confirmation

`cargo test -p ma-capture leak_erl_from_paired_fixture_tracks` (T0), which synthesises a known 18 dB return
loss and requires the computed value within 1 dB; `leak_measurement_reports_no_qualifying_window` (T0);
`cargo xtask manual-record --id v-win1-leak-live-per-app --require pass` (T2, windows tier).


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
