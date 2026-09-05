---
id: change-20260904-phase1-windows-detection-and-capture
kind: change
title: 'Phase 1: Windows detection and audio-capture PoC'
summary: Windows process, package, audio-session and microphone collectors, process
  loopback capture, a diagnostic harness in ma-engine, replayable recorded fixtures,
  the detection-only extension PoC and the process-tree corroboration join.
status: done
created: '2026-09-04'
updated: '2026-09-04'
profile: sdd@1
intent: Turn PLAN.md's Phase 1 deliverable list into implementable contracts on top
  of the Phase 0 seams, and close the gap the Phase 0 tree left between what the accepted
  ADRs promise and what the code does — most sharply that a determinate Google Meet
  start is supposed to require a microphone signal from the same browser process tree,
  and nothing joins on that key today.
outcomes:
- Windows collectors that produce process, package-identity, audio-session and microphone-use
  signals behind the existing SignalSource seam, with a stated precedence rule between
  their two evidence sources and a typed failure when the primary source is unavailable.
- A WASAPI capture source behind the existing CaptureSource trait with three typed
  activation outcomes, a system-loopback fallback that records its contamination,
  an always-available manual path, and a format pin that makes the thirty-second chunk
  cadence mean thirty seconds.
- A microphone track that opens the endpoint the meeting application is itself using,
  with the endpoint crossing from the collector to the capture engine as an argument
  from the composition root rather than as a crate edge the boundary check forbids.
- A composition root and diagnostic harness in the existing ma-engine crate that wires
  the collectors, the capture sources, the extension channel and the detector, appends
  a live session's timeline signal by signal, and exposes the confirmation-label command
  PLAN asks for.
- A detector that joins tab and microphone evidence on process_tree_root_pid, so a
  meeting tab in one browser window can no longer be corroborated by an unrelated
  call in a different process tree.
- Five recorded, redacted, replayable signal-timeline fixtures with confirmation-label
  and decisions sidecars, so Phase 2's validation matrix has real inputs and Phase
  1's diagnostics are inspectable without re-running the detector.
- A detection-only manifest-v3 browser extension PoC that can actually reach the loopback
  listener, provisioned by the harness, with permissions limited to the tabs API and
  the loopback host.
- A per-application echo return loss measured by one fixed method, and a per-application
  process-tree-loopback requirement set from a measured comparison rather than authored.
- A verification registry that holds more than one canonical plan without invalidating
  the previous phase's registrations, and a manual-verification record family that
  makes an observation the hosted runner cannot perform into a gate it can — including
  the declared required observations that stop a record from claiming pass while leaving
  most of its subject unobserved.
- The endpoint descriptor's owner-only DACL applied rather than merely constructed,
  and both trust-reversal observations that adr-20260903-extension-localhost-channel-trust
  assigns to Phase 1 recorded rather than skipped.
scope:
- Windows process and package-identity collector, audio-session and microphone-use
  collector, and the non-signal endpoint accessor, in crates/ma-signals-windows.
- WASAPI process-loopback capture source, system-loopback fallback, manual path, microphone
  endpoint selection and echo-leak measurement, in crates/ma-capture.
- Composition root, diagnostic harness and the ma-diag binary, in crates/ma-engine.
- The process-tree corroboration join and its two new rule ids, in crates/ma-detect.
- The detection-only extension PoC under extension/.
- Recorded fixtures with labels and decisions sidecars under fixtures/signal-timelines/.
- The endpoint-descriptor ACL application, the additive peer process-tree field on
  the channel server's request type, and the two trust-reversal observations, in crates/ma-ext-channel.
- The per-application process-tree-loopback requirement, recorded only in the Windows-tier
  measured comparison record and kept complete by the procedure's declared required
  observations; no adapter.toml or AdapterSpec field is added.
- Repository policy — the capture-path-isolation source list in boundary.toml, the
  multi-plan verification registry and the manual-verification procedure and record
  family in verification-tiers.toml, manual-verification.toml and xtask.
- Eight ADRs recording the decisions this phase closes.
non_goals:
- The session state machine, countdown, hysteresis, mode resolution and consent UI
  — Phase 2; Phase 1 adds no path by which a detector decision alone starts a recording.
- Detection thresholds and the per-application validation matrix — Phase 2 and Phase
  5; Phase 1 produces the fixtures that matrix will replay.
- Transcription, diarization and summarization — Phase 3.
- Real Google Drive and Notion clients — Phase 4.
- Tab audio capture, DOM access and content scripts in the extension — a PLAN section
  4 non-goal.
- Endpoint provisioning for a store-installed extension; Phase 1 provisions an unpacked
  PoC extension and the shipped mechanism belongs to the phase that builds the installer.
- A declared process-tree-loopback field on adapter.toml or AdapterSpec; Phase 1 records
  the requirement in the measured comparison record and defers the field to the phase
  that gives it a behavioural consumer.
- A Windows 10 compatibility path; Windows 11 is the only target, which is what makes
  the process-loopback activation type safe to assume present.
- macOS and video capture — Phase 6.
- Accepting the eight ADRs this change proposes; acceptance is an authority act reserved
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
  approval_evidence: 'consultation-phase1-20260904-1 (2026-09-04): all eight Phase
    1 ADRs accepted through the design skill consultation under the user''s delegated
    authority for technical dispositions (conductor decisions recorded in the design
    run''s consultation.json and consultation-dispositions.json); each ADR journal
    records the proposed -> accepted transition.'
  reasons:
  - Requires acceptance of eight proposed ADRs by the named decision makers before
    implementation starts, two of which change how an accepted ADR is realised — the
    detector join and the extension endpoint provisioning.
  - Applies a security descriptor that Phase 0 only constructed and records the two
    observations whose outcome can supersede an accepted ADR about a local listening
    endpoint on a product that records meetings.
  - Changes ma-detect, the crate Phase 0 declared fixed and pure, and changes the
    condition under which any browser meeting is treated as detected.
  - Makes nine PLAN Phase 1 exit criteria depend on manual observations performed
    on a Windows machine with all four target applications installed, which is a resourcing
    commitment as well as a design one.
members:
- role: requirements
  path: changes/change-20260904-phase1-windows-detection-and-capture/requirements.md
  required: true
- role: implementation
  path: changes/change-20260904-phase1-windows-detection-and-capture/implementation.md
  required: true
- role: verification
  path: changes/change-20260904-phase1-windows-detection-and-capture/verification.md
  required: true
promotion:
- target: design-module-boundaries
  section: invariants
  action: upsert
  item:
    id: INV-002
    statement: No capture-path crate reaches ma-workflow, ma-processor, ma-destination,
      ma-store or any adapter crate, and the enforced source list covers every capture-path
      crate including ma-signals-windows and ma-ext-channel (v-isolation-capture-path-edges,
      v-isolation-negative-fixture, v-win1-capture-path-sources-cover-collectors).
    enforcement: test
  reason: FR-114 widens the mechanically enforced capture-path-isolation source list
    to match this invariant's own wording, and adds a check that fails when the list
    stops covering every capture-path crate.
- target: design-module-boundaries
  section: responsibilities
  action: upsert
  item:
    id: RESP-005
    statement: Name the L5 composition root as the only place where the platform collectors,
      the capture engine, the extension channel and the adapter crates are linked
      together, with every adapter dependency renamed so no service identifier appears
      outside L4.
  reason: Phase 1 creates the repository's first real composition root in crates/ma-engine;
    without this responsibility recorded, a later change could place the same wiring
    in an L3 crate and produce an L3-to-L3 violation.
- target: design-recording-artifact-model
  section: invariants
  action: upsert
  item:
    id: INV-007
    statement: Every capture source delivers 16 kHz mono to the chunk writer; a source
      whose device format differs resamples or fails activation rather than opening
      a track whose origin rate differs from SAMPLE_RATE (v-win1-capture-origin-rate-pinned).
    enforcement: test
  reason: The chunk writer writes origin.sample_rate into the WAV header and CHUNK_SAMPLES
    means thirty seconds only at 16 kHz, so the first real device-backed source makes
    the format pin an invariant rather than an assumption.
- target: design-recording-artifact-model
  section: capabilities
  action: upsert
  item:
    id: cap:echo-leak-measurement
    uniqueness: global
  reason: Phase 1 introduces a per-application echo-return-loss measurement over paired
    tracks as a capability of the recording model, with one fixed method and three
    outcomes.
- target: design-session-lifecycle
  section: invariants
  action: upsert
  item:
    id: INV-006
    statement: A diagnostic or headless entry point starts capture only under an explicit
      operator command and never through the consent-surface, countdown or hysteresis
      path (v-win1-harness-requires-explicit-invocation).
    enforcement: test
  reason: Phase 1 adds the first entry point that can start capture outside the session
    state machine, so the guarantee that it does not become a second automatic-start
    path becomes a standing invariant rather than a phase-local promise.
- target: design-threat-model
  section: invariants
  action: upsert
  item:
    id: INV-005
    statement: The extension endpoint descriptor is written with its owner-only DACL
      applied to the file, never merely constructed (v-win1-endpoint-descriptor-acl-applied).
    enforcement: test
  reason: EndpointDescriptor::write returns a SecurityDescriptor that nothing applies,
    so the trust-reversal observation NFR-103(a) makes would report the absence of
    a mechanism rather than a property of one.
- target: design-threat-model
  section: trust_boundaries
  action: upsert
  item:
    id: TB-002
    statement: 'browser extension to engine: non-authoritative tab signals cross the
      loopback channel under a token-authenticated listener with pinned origin and
      freshness window; a determinate start additionally requires an operating-system
      microphone signal whose process tree root equals the tab signal''s, and the
      Phase 1 proof-of-concept extension is provisioned by the diagnostic harness
      rather than by reading the endpoint descriptor.'
  reason: Phase 1 implements the same-process-tree clause for the first time and fixes
    how the proof-of-concept extension learns the endpoint, both of which change what
    crosses this boundary and under what conditions.
unresolved_decisions: []
tags:
- phase-1
- windows
- capture
- detection
- extension
owners:
- take
relations:
- {type: introduces, target: adr-20260904-windows-audio-signal-observation-apis}
- {type: introduces, target: adr-20260904-mic-endpoint-observed-outside-the-signal-envelope}
- {type: introduces, target: adr-20260904-echo-leak-measurement-representation}
- {type: introduces, target: adr-20260904-per-application-loopback-requirement-record}
- {type: introduces, target: adr-20260904-windows-rs-crate-and-gnu-fidelity}
- {type: introduces, target: adr-20260904-extension-endpoint-provisioning-poc}
- {type: introduces, target: adr-20260904-verification-registry-multi-plan-and-manual-records}
- {type: introduces, target: adr-20260904-detector-process-tree-corroboration-join}
- {type: dependsOn, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
evidence_refs:
- type: command
  ref: cargo xtask verify --tier portable --strict (OK on delivery/phase1-windows-detection-and-capture
    @ d4a0caf6; 36 portable Phase 1 verifications passed)
- type: command
  ref: 'cargo xtask verify --check-registration (OK: 158 plan verification ids, 158
    registrations, 30 windows)'
- type: command
  ref: 'cargo xtask boundary (OK: no boundary violations, 21 crates)'
- type: command
  ref: 'cargo xtask docs-check (OK: 5 rules)'
- type: command
  ref: cargo test --workspace and cargo clippy --workspace --all-targets -- -D warnings
    (green on Linux); cargo clippy --target x86_64-pc-windows-gnu for ma-signals-windows,
    ma-capture, ma-ext-channel and ma-engine (green, compile-check only)
- type: contract
  ref: verification-tiers.toml (46 Phase 1 registrations; the 10 windows-tier verifications
    are gated by cargo xtask manual-record and run by the ci.yml windows job once
    the manual records exist)
- type: contract
  ref: manual-verification.toml (9 procedures with required observations; no record
    committed yet)
- type: source
  ref: fixtures/signal-timelines/{teams-desktop-session,slack-huddle-session,zoom-desktop-session,meet-chrome-with-extension,meet-chrome-without-extension}.{jsonl,labels.json,decisions.json}
promotion_applied_at: '2026-09-04T04:49:44.534138+00:00'
closure:
  closed_at: '2026-09-04T04:49:45.851622+00:00'
  content_hash: sha256:331c348105d1284a61c0470a037f7f440228034dc9f3723738af2b12be4ae8a5
---

## Summary

Phase 0 produced seams and contracts; Phase 1 fills them with real Windows behaviour and, in three places,
repairs the gap between what the accepted decisions say and what the landed code does.

The sharpest of those is detection. `adr-20260903-extension-localhost-channel-trust` states normatively that a
determinate browser start "additionally requires an operating-system microphone signal whose subject process
belongs to the same browser process tree", and `Payload.process_tree_root_pid` exists with the doc comment
"so tab and microphone facts can be joined" — but nothing reads it. `decide()` keys candidates by adapter id
alone, so today a Google Meet tab in one Chrome window is corroborated by any microphone-using call in any
other Chrome process tree. This change implements the join, assigns both producers, and adds a cross-tree
fixture that must yield `Inconclusive`.

The second is ownership. Four Phase 1 contracts delegated wiring to "the composition root" and four more
presumed a "diagnostic harness", and neither existed: `ma-engine` depended on none of the signal, detector,
capture, collector or adapter crates. `ma-engine` is the L5 crate the boundary check exempts from the layering
rule precisely so that it can link everything, so the harness lands there, and with it the live session
lifecycle, the incremental timeline append that keeps a crashed session's signals, the confirmation-label
command PLAN asks for, and the decisions sidecar that makes diagnostics readable without running the detector.

The third is verification honesty. The hosted Windows runner has no Teams, Slack, Zoom or Chrome installed, no
speaker and no microphone, yet six exit criteria need exactly those and two more had no command at all. Rather
than let them pass vacuously, this change extends the registry to hold both phases' plans and adds a
manual-verification family: a declared procedure, a committed record, and a digest pin so that editing a
procedure invalidates every record taken against the old text.

Three decisions the design draft left open are closed against their own drafted defaults, because leaving them
open would have let two conforming implementations produce incomparable results. The echo measurement is one
echo-return-loss number over one qualifying sixty-second window, recorded outside the signal envelope rather
than smuggled into the shared `Payload.level_dbfs` field. The microphone endpoint likewise leaves the envelope:
`Subject` is a closed union and no detector rule reads an endpoint, so it travels as an argument from the
composition root instead of as a schema field or an illegal crate edge. And the per-application
process-tree-loopback requirement stays in the Windows-tier comparison record that measures it, instead of
being copied into an `adapter.toml` field that four L4 crates, the shared conformance suite and the composition
root read but that no Phase 1 behaviour consults; the procedure's declared required observations, not a second
copy of the fact, are what keep the record complete for every application.

`requirements.md` carries the EARS requirements and the acceptance criteria, `implementation.md` the contracts,
seams and unit order, and `verification.md` the tiered checks, the manual-record gate and what would falsify
this change.

## Closure Notes

## Consultation resolutions

consultation-phase1-20260904-1 (2026-09-04) closed the two authority questions the plan raised:

- Extension transport direction: the accepted loopback channel stays; Phase 1 collects the two reversal observations (NFR-103) instead of moving to native messaging now. A confirmed reversal is raised as an ADR supersede for consultation.
- Manual verification as a merge gate: the nine manual-record identifiers remain the gate; no self-hosted Windows runner is assumed. A runner with the four applications installed would convert six of the nine into registered commands without a design change.


{% transition from="draft" to="ready" date="2026-09-04" %}
design run design-phase1-20260904-1 finalized: canonical plan materialized, 8 ADRs accepted
{% /transition %}


{% transition from="ready" to="active" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: 14 task commits on delivery/phase1-windows-detection-and-capture
{% /transition %}


{% transition from="active" to="closing" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: all 14 tasks done, evidence recorded
{% /transition %}
