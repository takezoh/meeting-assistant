---
change: change-20260904-phase1-windows-detection-and-capture
role: requirements
functional_requirements:
- id: FR-101
  statement: The Windows signal collector shall observe process lifecycle and package
    identity for Teams Desktop, Slack Huddle, Zoom Desktop, and the browser processes
    hosting Google Meet, using only OS process- and package-enumeration APIs, and
    shall emit ProcessStarted, ProcessStopped, and PackageIdentityObserved signals
    carrying Subject::Process{pid, image_name, package_family_name} with no window
    title, control label, or DOM-derived text in any field.
  priority: must
- id: FR-102
  statement: The Windows signal collector shall observe per-process audio-session
    lifecycle and microphone-capture state for the same four target applications and
    shall emit AudioSessionCreated, AudioSessionDestroyed, MicCaptureStarted, and
    MicCaptureStopped signals attributed to the owning process, from the session-manager
    notification source only, within a one-second observation-latency bound measured
    on that primary source; the consent-store poll shall corroborate and shall never
    by itself cause a signal to be emitted.
  priority: must
- id: FR-103
  statement: When a target application's process is capturing audio and process-specific
    (including process-tree) loopback activation succeeds, ma-capture shall provide
    a CaptureSource whose TrackOrigin records capture_mode = ProcessLoopback, contamination_risk
    = None, sample_rate = 16000 and channels = 1 for that application's meeting-audio
    track.
  priority: must
- id: FR-104
  statement: If process-specific loopback activation is unavailable or fails for a
    target application, ma-capture shall fall back to system (default-endpoint) loopback
    capture, recording capture_mode = SystemLoopback and contamination_risk = PossibleOtherApps
    on the resulting track, and a manual-start capture path (Device-mode CaptureSource
    selected by explicit user action) shall remain available independent of the loopback
    outcome.
  priority: must
- id: FR-105
  statement: When a target application has an active audio-capture session on a specific
    microphone endpoint, the system shall open that same endpoint for the recorded
    microphone track rather than the system default communications device, taking
    the endpoint identifier as an explicit input supplied by the composition root
    from the audio-session collector's observation rather than through the signal
    envelope, and shall re-evaluate the selection through the existing SourceEvent::FormatChanged
    / successor-track path when the meeting application's endpoint changes mid-session.
  priority: must
- id: FR-106
  statement: While a target application is recording on a speaker (non-headphone)
    audio path, the system shall compute the echo return loss between the loopback
    track and the concurrently captured microphone track as the difference of their
    root-mean-square levels in dBFS over one qualifying sixty-second window, and shall
    record that value per application together with the window's position on both
    tracks, both level values and the session's alignment uncertainty, or shall record
    an explicit no-qualifying-window or inconclusive-alignment outcome instead of
    a number.
  priority: must
- id: FR-107
  statement: The system shall record, per target application and independent of any
    single recording, whether process-tree loopback rather than single-process loopback
    is required to capture that application's meeting audio completely, and that recorded
    value shall be derived from a measured comparison of the two activation modes
    against the same application rather than authored by hand.
  priority: must
- id: FR-108
  statement: Every Phase 1 diagnostic session's signal timeline shall be appended
    to durable storage as each signal is observed, using the existing TimelineHeader-plus-JSONL
    fixture shape already committed under fixtures/signal-timelines/, so that a session
    ended by stop, by cancel or by a crash retains the signals observed before that
    point, and each persisted fixture shall be replayable by the existing detector-replay
    test path without modification to that path.
  priority: must
- id: FR-109
  statement: The system shall provide the person conducting a Phase 1 diagnostic session
    an explicit command that records a "was this a meeting?" confirmation for one
    or more time ranges of a captured timeline, persisted using the existing <timeline>.labels.json
    sidecar shape (timeline, labels[{from_monotonic_ns, to_monotonic_ns, was_meeting,
    note}]).
  priority: must
- id: FR-110
  statement: The detection-only browser extension shall report the active tab's host
    and audible state for Google Meet tabs to the desktop application over the existing
    localhost extension channel, using only the fields already defined by ExtensionMessage
    and only the browser's tabs API from its background service-worker context, and
    shall obtain the listener's port and per-start token from an endpoint file that
    the diagnostic harness writes into the unpacked extension directory, never from
    a filesystem, nativeMessaging or broad-host permission.
  priority: must
- id: FR-111
  statement: The detector shall treat an extension tab signal and an operating-system
    microphone-use signal as corroborating each other only when both carry payload.process_tree_root_pid
    and the two values are equal, and shall otherwise return Inconclusive with a rule
    identifier naming whether the join key was absent or mismatched, so that no determinate
    Google Meet start is produced from extension tab evidence alone or from microphone
    use in a different browser process tree.
  priority: must
- id: FR-112
  statement: When a Phase 1 diagnostic session ends, the system shall write the detector's
    decision output for that session's timeline to a committed <timeline>.decisions.json
    sidecar citing the signal identifiers and adapter rule id of every decision, so
    that the diagnostics are inspectable per session without re-running the detector.
  priority: must
- id: FR-113
  statement: A Phase 1 capture session against any of the four target applications
    shall complete a two-hour recording through the existing chunk-writer and manifest
    durability path without data loss, as measured on the Windows verification tier.
  priority: must
- id: FR-114
  statement: The capture-path-isolation rule in boundary.toml shall name ma-signals-windows
    and ma-ext-channel as additional sources, alongside the existing ma-core-types,
    ma-session and ma-capture, so the mechanically enforced rule matches the module-boundaries.md
    INV-002 wording that no capture-path crate reaches ma-workflow, ma-processor,
    ma-destination, ma-store, or any adapter crate.
  priority: must
- id: FR-115
  statement: When a Phase 1 collector starts or restarts while a condition it observes
    is already true, it shall emit CollectorStarted as its first signal and shall
    set payload.restart_resync on the first signal it emits for that already-true
    condition, so the detector's existing resync-no-autostart rule downgrades it instead
    of treating a collector restart as a fresh meeting start.
  priority: must
- id: FR-116
  statement: The verification registry shall declare every canonical plan whose verification
    identifiers it holds and shall treat the union of those plans' declared identifiers
    as the registered set, so that adding Phase 1's plan does not make any Phase 0
    registration stale and no plan's identifier can be registered twice.
  priority: must
- id: NFR-101
  statement: Every Phase 1 verification that requires real Windows OS behavior, a
    real target application, or real Chrome/Edge extension policy shall be registered
    with platform = windows in verification-tiers.toml and shall run in the existing
    CI windows job; no such verification shall be treated as satisfied by a portable-tier
    run alone.
  priority: must
- id: NFR-102
  statement: Every Windows-only dependency shall be declared under [target.'cfg(windows)'.dependencies]
    against a single workspace-level pinned version, every WASAPI or COM call site
    shall be behind a cfg(windows) attribute, with a portable fake backend behind
    the same trait, and cargo test --workspace and cargo clippy --workspace --all-targets
    -- -D warnings shall stay green on the ubuntu portable job after Phase 1 lands.
  priority: must
- id: NFR-103
  statement: The endpoint descriptor writer shall apply the owner-only security descriptor
    it already builds to endpoint.json before the file is used, and Phase 1 shall
    add Windows-tier verifications that observe (a) whether that applied DACL in fact
    prevents another same-user process from reading the token and (b) whether Chrome/Edge
    policy permits the detection-only extension to reach the loopback listener; the
    localhost-channel design in adr-20260903-extension-localhost-channel-trust shall
    not be superseded until both observations are recorded, and either observation
    showing a violation of the intended trust model shall be raised as a decision
    open for consultation rather than silently continuing.
  priority: must
- id: NFR-104
  statement: Every new Windows-observed datum shall be represented using an existing
    SignalKind, Subject or Payload field, or shall be carried outside the signal envelope
    entirely as capture-side or diagnostic data; introducing a new field or variant
    into the closed schema requires a schema_version bump proposed as an ADR, never
    an ad hoc addition.
  priority: must
- id: NFR-105
  statement: The Phase 1 diagnostic harness shall start capture only under an explicit
    operator-supplied subcommand, shall neither construct nor consult ConsentSurfaces,
    the countdown or the hysteresis path, and shall add no code path by which a detector
    decision alone can start a recording.
  priority: must
- id: NFR-106
  statement: Every Phase 1 verification whose observation cannot be made by an unattended
    hosted runner shall declare a manual verification procedure with a named owner,
    host profile, steps, artifact, pass criterion and the set of observations its
    record must carry, and its registered command shall fail when the corresponding
    record is absent, records an outcome other than pass, omits an observation the
    procedure declares as required, or was produced against a different revision of
    the procedure.
  priority: must
---

<!-- lifecycle is owned by change.md -->

# Requirements

Phase 1 turns PLAN.md section 6's Phase 1 deliverables and exit criteria into requirements that the contracts
in `implementation.md` discharge and the checks in `verification.md` falsify. The canonical mapping between
requirement, contract, unit and check lives in the design plan copied into `design-plan/`.

## Scope

Windows process, package-identity, audio-session and microphone-use collection; process-specific loopback
capture with a system-loopback fallback and a manual path; microphone endpoint selection that follows the
meeting application's own session; per-application echo measurement; the per-application process-tree-loopback
record; a diagnostic harness and composition root; replayable recorded fixtures with confirmation-label and
decisions sidecars; the detection-only browser extension proof of concept; the process-tree corroboration join
in the detector; and the repository policy that makes all of it verifiable — the capture-path-isolation source
list, a multi-plan verification registry and the manual-verification record family.

Out of scope: the session state machine, countdown, hysteresis and consent user interface (Phase 2); detection
thresholds and the validation matrix (Phase 2 and Phase 5); transcription and summarization (Phase 3); real
destination clients (Phase 4); tab audio capture, DOM access and content scripts, which are PLAN section 4
non-goals; endpoint provisioning for a store-installed extension, which belongs to the phase that builds the
installer; a Windows 10 compatibility path; and macOS and video capture (Phase 6).

## Functional requirements (EARS)

- **FR-101** (must) The Windows signal collector shall observe process lifecycle and package identity for Teams
  Desktop, Slack Huddle, Zoom Desktop and the browser processes hosting Google Meet, using only operating-system
  process- and package-enumeration interfaces, and shall emit `ProcessStarted`, `ProcessStopped` and
  `PackageIdentityObserved` signals carrying `Subject::Process{pid, image_name, package_family_name}` with no
  window title, control label or DOM-derived text in any field.
- **FR-102** (must) When a target application's audio-capture session changes state, the system shall emit the
  corresponding `AudioSessionCreated`, `AudioSessionDestroyed`, `MicCaptureStarted` or `MicCaptureStopped`
  signal attributed to the owning process within one second, from the session-manager notification source only;
  the consent-store poll shall corroborate and shall never by itself cause a signal to be emitted.
- **FR-103** (must) When a target application's process is capturing audio and process-specific (including
  process-tree) loopback activation succeeds, the system shall provide a `CaptureSource` whose `TrackOrigin`
  records `capture_mode = ProcessLoopback`, `contamination_risk = None`, `sample_rate = 16000` and
  `channels = 1`.
- **FR-104** (must) If process-specific loopback activation is unavailable or fails for a target application,
  then the system shall fall back to system (default-endpoint) loopback capture recording
  `capture_mode = SystemLoopback` and `contamination_risk = PossibleOtherApps`, and a manual-start Device-mode
  capture path shall remain available independent of the loopback outcome.
- **FR-105** (must) While a target application holds an active audio-capture session on a specific microphone
  endpoint, the system shall open that endpoint for the recorded microphone track rather than the system default
  communications device, taking the endpoint identifier as an explicit input supplied by the composition root
  from the collector's observation rather than through the signal envelope, and shall re-evaluate the selection
  through the existing `SourceEvent::FormatChanged` and successor-track path when the endpoint changes
  mid-session.
- **FR-106** (must) While a target application is recording on a speaker (non-headphone) audio path, the system
  shall compute the echo return loss between the loopback and microphone tracks as the difference of their
  root-mean-square levels in dBFS over one qualifying sixty-second window, and shall record it per application
  together with both levels, the window's start sample on each track and the session's alignment uncertainty, or
  shall record an explicit `no_qualifying_window` or `inconclusive_alignment` outcome instead of a number.
- **FR-107** (must) The system shall record, per target application and independently of any single recording,
  whether process-tree loopback rather than single-process loopback is required to capture that application's
  meeting audio completely, derived from a measured comparison of the two activation modes against the same
  application rather than authored by hand.
- **FR-108** (must) When a diagnostic session observes a signal, the system shall append it to that session's
  durable timeline in the existing `TimelineHeader`-plus-JSONL fixture shape before reading the next signal, so
  that a session ended by stop, by cancel or by a crash retains every signal observed up to that point and
  remains replayable by the existing detector-replay path without modification to that path.
- **FR-109** (must) The system shall provide the person conducting a diagnostic session an explicit command that
  records a "was this a meeting?" confirmation for one or more time ranges of a captured timeline, persisted in
  the existing `<timeline>.labels.json` sidecar shape.
- **FR-110** (must) When a Google Meet tab is active in the browser, the detection-only extension shall report
  that tab's host and audible state to the desktop application over the existing localhost channel using only
  the fields `ExtensionMessage` already defines and only the browser tabs interface, obtaining the listener's
  port and per-start token from the endpoint file the diagnostic harness writes into the extension's own
  directory and never from a filesystem, native-messaging or broad-host permission.
- **FR-111** (must) The detector shall treat an extension tab signal and an operating-system microphone-use
  signal as corroborating each other only when both carry `payload.process_tree_root_pid` and the values are
  equal, and shall otherwise return `Inconclusive` with a rule identifier naming whether the join key was absent
  or mismatched.
- **FR-112** (must) When a diagnostic session ends, the system shall write the detector's decision output for
  that session's timeline to a committed `<timeline>.decisions.json` sidecar citing the signal identifiers and
  adapter rule identifier of every decision.
- **FR-113** (must) The system shall complete a two-hour capture session against a target application through the
  existing chunk-writer and manifest durability path without data loss, as measured on the Windows verification
  tier.
- **FR-114** (must) The capture-path-isolation rule shall name `ma-signals-windows` and `ma-ext-channel` as
  sources alongside `ma-core-types`, `ma-session` and `ma-capture`, so the enforced rule matches the documented
  INV-002 wording.
- **FR-115** (must) When a collector starts or restarts while a condition it observes is already true, the system
  shall emit `CollectorStarted` first and shall set `payload.restart_resync` on the first signal it emits for
  that already-true condition.
- **FR-116** (must) The verification registry shall declare every canonical plan whose verification identifiers
  it holds and shall treat the union of those plans' identifiers as the registered set.

## Non-functional requirements

- **NFR-101** (must) Every verification requiring real Windows behaviour, a real target application or real
  browser policy is registered with `platform = windows` and runs in the CI windows job; none is satisfied by a
  portable-tier run alone.
- **NFR-102** (must) Every Windows-only dependency is declared under `[target.'cfg(windows)'.dependencies]`
  against a single `[workspace.dependencies]` pin, every WASAPI or COM call site is gated with a portable fake
  behind the same trait, and `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
  stay green on the ubuntu portable job.
- **NFR-103** (must) `EndpointDescriptor::write` applies the owner-only security descriptor it builds before the
  file is used, and both trust-reversal observations are recorded before the localhost-channel decision may be
  superseded; an observation showing a violation is raised as a decision open for consultation.
- **NFR-104** (must) Every new Windows-observed datum uses an existing `SignalKind`, `Subject` or `Payload`
  field, or is carried outside the signal envelope as capture-side data; a new field or variant requires an
  ADR-gated `schema_version` bump.
- **NFR-105** (must) The diagnostic harness starts capture only under an explicit operator subcommand, consults
  no `ConsentSurfaces`, countdown or hysteresis path, and adds no path by which a detector decision alone starts
  a recording.
- **NFR-106** (must) Every verification whose observation an unattended hosted runner cannot make declares a
  manual procedure with an owner, host profile, steps, artifact, pass criterion and the observations its record
  must carry, and its registered command fails when the record is absent, not `pass`, missing a declared
  required observation, or stale against the procedure.

## Invariant requirements

These hold at every moment of Phase 1, not only at a transition, and each is the negation of a way the phase
could look finished while being wrong.

- **INV-P1-1** No Phase 1 datum adds a field or a variant to `Signal`, `Subject` or `Payload`. Two facts the
  phase observes — the microphone endpoint and the echo measurement — leave the envelope entirely rather than
  entering it.
- **INV-P1-2** `ma-capture` never depends on `ma-signals-windows` and vice versa. Both are L3; every fact that
  crosses between them crosses through the L5 composition root as plain data.
- **INV-P1-3** No committed fixture carries a real machine profile, pid, image name, package family name or tab
  host. `machine_profile` is exactly `"redacted"` and identifiers are synthetic under a documented mapping.
- **INV-P1-4** A capture source never delivers samples at a rate other than `SAMPLE_RATE` to the chunk writer;
  it resamples or refuses to open.
- **INV-P1-5** There is no code path from a `decide()` outcome to a `CaptureSource` in Phase 1. Detection
  explains; the operator records.
- **INV-P1-6** Every verification identifier this plan declares is registered exactly once across the declared
  plans, and every Phase 0 identifier stays registered.

## Delta against the upstream decisions

Three of the conductor's dispositions of 2026-09-04 are carried unchanged: the process-loopback activation
choice, the microphone-use observation choice, and the `align-config-to-doc` widening of the
capture-path-isolation rule. The verify-in-phase treatment of the extension trust-reversal condition is also
carried, and is strengthened by the fact that Phase 1 now applies the descriptor whose readability the
observation measures.

Three questions the design draft carried as open are closed here rather than deferred, because leaving them
open would let two conforming implementations produce incomparable results. The echo measurement's
representation is closed *against* the draft's own candidate: it leaves the signal envelope rather than reusing
`Payload.level_dbfs`. The per-application loopback-requirement location is likewise closed *against* the
draft's candidate: the requirement is recorded only in the Windows-tier measured comparison record, no
`adapter.toml` or `AdapterSpec` field is added, and the procedure's declared required observations are what
keep the record complete for every application. The Windows bindings question is closed as a single
workspace-level pin with target gating, and the GNU cross-check is deleted as a merge gate rather than made
conditional.

Two requirements exist only because the design critique found an accepted decision with no implementation and a
producer with no owner: FR-111's join, which `adr-20260903-extension-localhost-channel-trust` has required since
Phase 0 while nothing read the join key, and FR-115's resync flag, without which the accepted
`resync-no-autostart` rule is inert because nothing ever sets it.

## Acceptance criteria

`A-01` through `A-09` map one-to-one onto PLAN section 6 Phase 1's nine exit-criterion bullets; `A-10` through
`A-14` cover five obligations that had no exit criterion. Full text is in the design plan's `spine.yaml`.

| id | criterion | requirements |
| --- | --- | --- |
| A-01 | Meeting audio and microphone audio both recorded for at least one target application, from a real Windows recording | FR-103, FR-104 |
| A-02 | Meet start and end produced only from same-process-tree corroboration; a cross-tree fixture yields inconclusive; no DOM access anywhere | FR-110, FR-111 |
| A-03 | Two-hour recording completes with manifest matching directory and zero unexplained gaps, and the same accounting holds for a synthetic run on the portable tier | FR-113 |
| A-04 | Start and end signals observable in a recorded fixture for all four targets, with each fixture's committed decisions sidecar citing signals and rule ids | FR-101, FR-102, FR-112 |
| A-05 | No window title, control label, DOM-derived value or new field anywhere in the closed schema | FR-101, NFR-104 |
| A-06 | Contamination outcomes and the per-application process-tree-loopback requirement documented in the Windows-tier comparison record, which the gate rejects unless it carries one observation per adapter table | FR-104, FR-107 |
| A-07 | Recorded microphone endpoint matches the meeting application's session across at least one endpoint change, with no crate edge between the two L3 crates | FR-105 |
| A-08 | A per-application echo return loss, or an explicit non-measurement outcome, recorded for each target application by the one fixed method | FR-106 |
| A-09 | Every fixture replays byte-identically against its sidecar, carries a confirmation label, and a cancelled session keeps its partial timeline | FR-108, FR-109 |
| A-10 | A collector restarted mid-meeting produces a resync-flagged first signal that the existing rule downgrades | FR-115 |
| A-11 | Registration passes across both plans, every T2 in the windows tier, every manual id backed by a non-stale passing record, boundary rule green over the widened source list | FR-114, FR-116, NFR-101, NFR-106 |
| A-12 | Workspace test and lint green on the ubuntu portable job, with the gating check confirming no ungated Windows dependency | NFR-102 |
| A-13 | Endpoint descriptor written with its DACL applied, and both trust observations recorded with a stated outcome | NFR-103 |
| A-14 | The harness starts no capture without its explicit subcommand and references no consent-surface, countdown or hysteresis symbol | NFR-105 |

## Non-goals for this change

Phase 1 does not build the session state machine, the consent surface, the workflow runtime or any destination;
it does not set detection thresholds or build the validation matrix; it does not capture tab audio, read the
DOM or inject a content script; it does not decide endpoint provisioning for a store-installed extension; it
does not support Windows 10; and it does not accept the eight ADRs it proposes.
