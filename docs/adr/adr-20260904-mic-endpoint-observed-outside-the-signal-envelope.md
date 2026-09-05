---
id: adr-20260904-mic-endpoint-observed-outside-the-signal-envelope
kind: adr
title: The meeting application's microphone endpoint is capture data, not a signal
summary: The per-process capture endpoint is exposed by the Windows collector through
  a non-SignalSource accessor and passed to ma-capture as an argument, rather than
  entering the closed signal envelope.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - The closed Signal, Subject and Payload shapes stay unchanged, so no schema_version
    bump and no fixture migration is needed for a fact no detector rule reads.
  - ma-capture and ma-signals-windows stay independent L3 crates, because the endpoint
    crosses as a string argument from the L5 composition root rather than as a crate
    edge the boundary check would reject.
  - The endpoint fact keeps a single consumer and a single producer, so its staleness
    window is the harness loop rather than the replayable timeline's lifetime.
  negative:
  - The endpoint a recording used is not visible in a replayed signal timeline, so
    a fixture alone cannot explain why a particular microphone was opened.
  - A second consumer of the endpoint fact in a later phase would have to be wired
    through the composition root as well, rather than subscribing to a signal.
  neutral:
  - TrackOrigin already records the format the selected endpoint produced, so the
    recording artifact still carries the consequence of the choice even though the
    timeline does not carry its cause.
  - The accessor is a second public surface on the collector crate beside SignalSource,
    which the crate did not previously have.
confirmation: cargo test -p ma-capture mic_endpoint_follows_supplied_session_endpoint
  (T0) and endpoint_change_opens_successor_track (T0); cargo test -p ma-signal payload_and_subject_field_sets_are_unchanged
  (T0); cargo xtask boundary (T0).
tags:
- windows
- audio
- signals
- boundaries
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
source_paths:
- crates/ma-signal/src/envelope.rs
- crates/ma-capture/src/source.rs
- boundary.toml
updated: '2026-09-05'
---

## Context

`FR-105` requires the recorded microphone track to use the endpoint the meeting application's own capture
session is bound to, rather than the system default communications device. The fact therefore has to travel
from the Windows audio-session collector to `ma-capture`.

Two repository facts constrain how. First, `Subject` is a closed four-variant tagged union
(`Process`, `Device`, `Tab`, `System`) asserted with `additionalProperties: false` by
`schema_contains_no_free_text_subject`, and a `Signal` carries exactly one `Subject`; `Payload` has six typed
fields and none of them is an endpoint. So a `MicCaptureStarted` attributed to `Subject::Process` — which
`FR-102` requires — cannot also name a `Device`. Second, `ma-capture` and `ma-signals-windows` are both L3 in
`boundary.toml`, and `xtask/src/boundary.rs` allows an edge only when `dep_rank < rank` for a layer with no
`edges.restricted` entry, so a direct dependency between them is a violation.

The design draft required `ma-capture` to consume "the audio-session collector's per-process session/endpoint
data" without saying through what, which left three mutually exclusive implementations open: add a schema
field, add a crate edge, or invent a side channel.

## Decision

The endpoint is **capture configuration, not detection evidence**, and it leaves the signal envelope entirely.

The audio-session collector exposes the per-process capture endpoint through an accessor that is not part of
`SignalSource` and that produces no `Signal`. The composition root (`crates/ma-engine`, layer L5, which may
depend on any lower layer) reads that accessor and passes the endpoint identifier into `ma-capture`'s
microphone selection as an ordinary `Option<&str>` argument. `ma-capture` names no type, trait or dependency
from `ma-signals-windows`. A mid-session endpoint change is delivered as a new hint and re-evaluated through
the existing `SourceEvent::FormatChanged` and `TrackSegment::open_successor` path.

The test of whether a fact belongs in the envelope is whether a detector rule reads it. No rule reads an
endpoint; `decide()` never inspects `Subject::Device`.

## Alternatives considered

**An ADR-gated `Payload.endpoint_id` bump.** `NFR-104` permits a new field with a `schema_version` bump and an
ADR, so this was available. Rejected because the field would be a permanent addition to a schema shared by four
crates, the JSON Schema under `contracts/`, the conformance suite and every committed fixture, in service of a
fact no detector consumes; and because a `schema_version` bump obliges the fixture-upgrade work that
`adr-20260903-detector-signal-replay-contract` scopes to a real envelope revision, not to a capture setting.

**A paired `Subject::Device` signal correlated by time.** Emitting `AudioSessionCreated` twice — once on the
process subject and once on the device subject — needs no schema change. Rejected because the two signals carry
no join key: `SignalTimeline` orders by monotonic time only, `Subject::key()` produces disjoint keys for the
two variants, and two applications switching endpoints in the same second would be indistinguishable.

**Routing the fact through `ma-signal` as a non-`Signal` type.** Legal by layer, since both L3 crates may
depend on L1. Rejected because it would put a Windows-specific audio concept into the crate that owns the
platform-neutral signal contract, for the sole purpose of evading a crate-edge rule that the composition root
already satisfies legitimately.

## Consequences

**Positive.**

- The closed schema is unchanged, so no bump, no fixture migration and no conformance-suite churn.
- The two L3 crates stay independent and `cargo xtask boundary` stays green without an exemption.
- The fact has one producer and one consumer, so its staleness window is one harness loop iteration rather than
  the lifetime of a persisted timeline.

**Negative.**

- A replayed signal timeline cannot explain which microphone was opened or why, so a fixture alone is not a
  complete account of a recording; the manual verification record for `v-win1-mic-endpoint-live` is.
- A later phase that needs the endpoint elsewhere must be wired through the composition root rather than
  subscribing to a signal, which is more work than adding a consumer to a broadcast.

**Neutral.**

- `TrackOrigin` still records the format the selected endpoint produced, so the recording artifact carries the
  consequence of the choice even though the timeline does not carry its cause.
- `ma-signals-windows` gains a second public surface beside `SignalSource`, which it did not previously have.

## Confirmation

`cargo test -p ma-capture mic_endpoint_follows_supplied_session_endpoint` (T0) and
`endpoint_change_opens_successor_track` (T0); `cargo test -p ma-signal
payload_and_subject_field_sets_are_unchanged` (T0), which fails if an endpoint field is added later;
`cargo xtask boundary` (T0), which fails on an `ma-capture` to `ma-signals-windows` edge.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
