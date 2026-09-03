---
id: adr-20260903-audio-format-and-chunking
kind: adr
title: 16 kHz mono WAV chunks during capture, verified FLAC consolidation afterwards
summary: Each track is written as 30-second 16 kHz mono WAV chunks made durable by
  atomic rename, then consolidated to FLAC only after a sample-identical verification.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - The maximum audio loss on any abrupt termination is a stated, tested 30 seconds.
  - A recovered session reports what it lost instead of silently presenting a shorter
    recording as complete.
  - A stalling or full disk degrades coverage visibly rather than stalling capture.
  negative:
  - Recording uses roughly twice the disk space until consolidation runs, and consolidation
    is extra work after every meeting.
  - A 30-second chunk cadence means a filesystem operation every 30 seconds per track
    for the whole meeting.
  - '16 kHz mono is a lossy product decision: the archive can never be used for anything
    needing more bandwidth.'
  neutral:
  - Gap records become a first-class part of the timeline that every downstream consumer
    has to handle.
  - Optional Opus export exists for sharing and is not the archival format.
confirmation: cargo test -p ma-engine --test durability kill_mid_chunk_bounded_loss
  (T2); cargo test -p ma-capture flac_decodes_sample_identical (T1) and directory_is_truth_manifest_is_cache
  (T1).
tags:
- audio
- durability
- data-model
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes per-track 16 kHz mono 16-bit WAV chunks of 30 seconds during recording, consolidation to FLAC per track after the meeting, and optional Opus for sharing. What it does not fix is the durability ordering, and that ordering is what decides how much audio a crash costs and whether a recovered session is honest about what it lost.

PLAN section 7 requires that an abrupt termination lose at most the in-progress chunk and that recovery be explicit rather than silent.

## Decision

Per track, samples accumulate into chunks of exactly 30 seconds except the last. A chunk becomes durable in this order: write to `<seq>.wav.part`, flush and `FlushFileBuffers`, rename to `<seq>.wav`, append a manifest record, fsync the manifest. Because the rename is the durability point, **at most one in-progress chunk of 30 seconds can be lost**, and that is the bound the contract tests.

The chunk **directory is the truth and the manifest is a cache**. On restart every present `<seq>.wav` is adopted, a manifest record naming an absent file becomes an explicit gap, and a `.part` file is repaired if it holds a complete frame and otherwise deleted and recorded as a gap. Sequence numbers are never renumbered: a missing sequence in the middle of a run is a gap, not a shift.

The chunk writer never blocks on the database, the control channel or the network. Disk stalls are absorbed by a bounded 60-second-per-track queue; on overflow the writer drops samples, records an explicit gap and emits a degraded event rather than stalling the audio callback. Losing bounded audio loudly is chosen over stalling the capture thread, which loses audio too but silently and without bound.

Consolidation verifies before it deletes: the decoded FLAC must be sample-identical to the chunk sequence with recorded gaps rendered as silence before any chunk file is removed, and a crash between verification and deletion re-runs idempotently.

## Alternatives considered

**Capture directly to Opus or FLAC.** Saves the consolidation step and disk space during recording. Rejected because a compressed stream truncated by a crash is far harder to recover than a truncated WAV, and because the recovery story is the reason the format was chosen.

**48 kHz stereo archival capture.** Higher fidelity and future-proof for other uses. Rejected because the product's consumers are speech transcription and diarization, which resample to 16 kHz mono anyway, and the storage cost is six times higher for no downstream benefit.

**Manifest as the truth with the directory as a cache.** Rejected because it makes a database or file write the durability point for audio that is already safely on disk, which inverts the risk.

**Delete chunks as soon as consolidation writes its output.** Rejected because an encoder bug would then be discovered after the source was gone.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- The maximum audio loss on any abrupt termination is a stated, tested 30 seconds.
- A recovered session reports what it lost instead of silently presenting a shorter recording as complete.
- A stalling or full disk degrades coverage visibly rather than stalling capture.

**Negative.**

- Recording uses roughly twice the disk space until consolidation runs, and consolidation is extra work after every meeting.
- A 30-second chunk cadence means a filesystem operation every 30 seconds per track for the whole meeting.
- 16 kHz mono is a lossy product decision: the archive can never be used for anything needing more bandwidth.

**Neutral.**

- Gap records become a first-class part of the timeline that every downstream consumer has to handle.
- Optional Opus export exists for sharing and is not the archival format.

## Confirmation

cargo test -p ma-engine --test durability kill_mid_chunk_bounded_loss (T2); cargo test -p ma-capture flac_decodes_sample_identical (T1) and directory_is_truth_manifest_is_cache (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
