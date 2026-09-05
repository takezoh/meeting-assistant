---
id: design-recording-artifact-model
kind: design
title: Recording artifact model
summary: Durable 30 s chunks, a sample-exact timeline with explicit gaps, verify-before-delete
  FLAC consolidation, root-relative artifact addressing and the two-phase delete that
  ends in a tombstone.
status: active
created: '2026-09-03'
scope_type: component
responsibilities:
- id: RESP-001
  statement: Turn captured samples into 30 s chunks whose durability point is the
    rename, so at most one in-progress chunk is lost on abrupt termination.
- id: RESP-002
  statement: Keep every sample's position in session coordinates with gaps as first-class
    records and per-track origins.
- id: RESP-003
  statement: Consolidate chunks to FLAC only after a sample-exact verification, recording
    every step in the manifest.
- id: RESP-004
  statement: Address every artifact relative to a root so a root can be relocated,
    and delete a meeting in two phases ending in a tombstone.
invariants:
- id: INV-001
  statement: A chunk's data file is renamed into place before its manifest record;
    the directory is the truth and the manifest a cache (v-chunk-manifest-vs-directory,
    v-chunk-kill-recovery).
  enforcement: test
- id: INV-002
  statement: A gap is always explicit, sequence numbers are dense-or-gapped and never
    renumbered, and a disk stall yields a gap and capture.degraded rather than a blocked
    callback (v-chunk-backpressure-gap).
  enforcement: test
- id: INV-003
  statement: The union of chunks and gaps tiles each track's range without overlap,
    and a format change opens a new segment with its own origin (v-timeline-coverage-invariant,
    v-timeline-format-change-segment).
  enforcement: test
- id: INV-004
  statement: WAV chunks are never deleted before a sample-exact verification of the
    FLAC that replaces them, and a crash at any point of encode, verify, rename, record,
    delete re-runs idempotently (v-consolidate-lossless, v-consolidate-crash-idempotent,
    v-consolidate-mismatch-keeps-chunks).
  enforcement: test
- id: INV-005
  statement: No artifact reference contains an absolute path or a user-chosen name,
    and relocating a root changes no reference (v-addressing-no-absolute-paths, v-addressing-relocation).
  enforcement: test
- id: INV-006
  statement: After a purge only the tombstone names the meeting, and a purge never
    coexists with an unresolved intended effect (v-purge-completeness, v-purge-idempotent,
    v-purge-cancels-inflight-steps).
  enforcement: test
- id: INV-007
  statement: Every capture source delivers 16 kHz mono to the chunk writer; a source
    whose device format differs resamples or fails activation rather than opening
    a track whose origin rate differs from SAMPLE_RATE (v-win1-capture-origin-rate-pinned).
  enforcement: test
boundaries:
  provides:
  - the chunk directory layout, manifest and schema under contracts/artifact
  - the timeline types in ma-core-types and the writer, recovery and consolidation
    in ma-capture
  - the artifact, roots and tombstone families in ma-store
  consumes:
  - samples from a CaptureSource per track
  - the filesystem under a declared artifact root
  forbidden:
  - deriving a timestamp from concatenation order
  - deleting bytes that have not been verified in their replacement form
  - a silent recreate of an artifact whose effect is unknown
variability:
  fixed:
  - 16 kHz mono s16le, 30 s chunks, the 60 s per-track queue
  - the durability order and the consolidation order
  - the two-phase delete and the tombstone
  free:
  - the FLAC encoder binding
  - the retention values that decide when a purge runs
capabilities:
- id: cap:durable-audio
  uniqueness: global
- id: cap:echo-leak-measurement
  uniqueness: global
failure_responsibilities:
- id: FR-001
  statement: A lost chunk is a gap record with its reason, visible to every consumer;
    it is never a shift of later positions.
- id: FR-002
  statement: A consolidation mismatch discards the FLAC, keeps the chunks as the archival
    form and marks consolidation_failed.
- id: FR-003
  statement: An artifact root that disappears mid-session fails the session after
    the bounded queue drains, with everything already durable preserved.
trust_boundaries:
- id: TB-001
  statement: 'capture callback to disk: the writer accepts samples without ever blocking
    on the filesystem, the database or the network.'
compatibility_policies:
- id: CP-001
  statement: The chunk manifest carries schema_version; a reader refuses a newer version
    rather than guessing.
- id: CP-002
  statement: capture_mode and contamination_risk exist per track so Phase 1 findings
    about loopback change data, not schema.
tags:
- audio
- artifacts
- durability
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- crates/ma-core-types/src/timeline.rs
- crates/ma-core-types/src/artifact_ref.rs
- crates/ma-capture/src/chunk_writer.rs
- crates/ma-capture/src/recovery.rs
- crates/ma-capture/src/consolidate.rs
- crates/ma-store/src/purge.rs
- contracts/artifact/chunk-manifest.schema.json
---

## Purpose

Audio is the one thing the product cannot regenerate. This document states how audio becomes durable, how
its positions stay exact through loss, how it is compacted without risk, how it is addressed, and how it is
deleted.

## Responsibilities

`ma-capture` owns the chunk writer, recovery and consolidation; `ma-core-types` owns the timeline and the
artifact reference; `ma-store` owns the artifact, roots and tombstone tables and the purge job.

## Boundaries

Layout under a root: `meetings/<meeting_id>/chunks/<track>/<seq:06>.wav` with `manifest.json` beside them,
`meetings/<meeting_id>/tracks/<track>.flac` after consolidation. References are `{root_id, segments}` where
every segment is a generated identifier or a typed name, so a relocated root changes no reference and no
reference can point outside its root. A track has exactly one origin: a device format or endpoint change
ends the track and opens a *successor track* with its own identifier, directory and sample space (the
manifest records `predecessor` / `successor`), so every consolidated file is `tracks/<track_id>.flac` and
every origin has one track row.

## Invariants

Durability order: `.part` → flush → rename → manifest record → fsync. Consolidation order: encode → verify →
rename → record → delete. Both orders are tested by crash injection at every point.

## Collaboration

The writer is fed by a `CaptureSource` per track (WASAPI in Phase 1, `SyntheticSource` in Phase 0), reports
`capture.degraded` through the engine, and hands the finalized manifest to consolidation. Deletion is
requested over `meeting.delete`, cancels in-flight workflow steps, waits for intended effects, and then
the purge job removes the meeting directory and rows and inserts the tombstone.

## Failure Responsibility

Loss is explicit, never silent; fidelity is never traded for bytes; a disappearing root fails the session
rather than the process.

## Variability

Fixed: format, chunk size, queue bound, both orders, the delete mechanism. Free: the encoder binding and
retention values.

## Conformance

`cargo test -p ma-capture` and `cargo test -p ma-core-types` on the portable tier; kill-mid-chunk, two-hour
recovery and interrupted purge on the Windows tier under `ma-engine`.

## Related Decisions

adr-20260903-audio-format-and-chunking, adr-20260903-local-store-and-artifact-layout,
adr-20260903-workflow-identity-and-idempotency.
