---
change: change-20260903-phase0-repository-and-contracts
role: requirements
functional_requirements:
- id: FR-001
  statement: The repository shall provide a cargo workspace whose crate graph declares
    the workflow core, service adapters, platform collectors, capture engine, and
    composition roots as separate crates with declared layer membership.
  priority: must
- id: FR-002
  statement: When the dependency-direction check runs and any crate outside a composition
    root depends directly, transitively, or through a feature gate on a service-adapter
    crate, the check shall exit non-zero and name the offending dependency path.
  priority: must
- id: FR-003
  statement: When a core-layer crate contains a supported-service identifier literal,
    the boundary check shall exit non-zero and name the file, line and literal.
  priority: must
- id: FR-004
  statement: While a session is in the recording state, if the UI process terminates,
    then the capture engine shall continue writing durable chunks and shall keep the
    session in the recording state.
  priority: must
- id: FR-005
  statement: When a client connects or reconnects to the capture engine, the engine
    shall return an authoritative session snapshot with a monotonically increasing
    event sequence number, and the client shall render that state rather than a locally
    inferred state.
  priority: must
- id: FR-006
  statement: If a client connecting to the engine control channel does not belong
    to the same operating-system user as the engine, then the engine shall refuse
    the connection before dispatching any method and shall record a security diagnostic
    containing no payload.
  priority: must
- id: FR-007
  statement: When the detector replays a recorded signal timeline with the same configuration
    and adapter-table version, it shall emit a byte-identical decision sequence across
    runs and processes, and every decision shall cite the signal identifiers it used.
  priority: must
- id: FR-008
  statement: Where no adapter matches the observed subject, or where a matched adapter's
    corroboration requirement is unmet, the detector shall return unknown or inconclusive
    respectively and the session shall fall back to manual control without arming.
  priority: must
- id: FR-009
  statement: When the browser extension reports a meeting tab without a corroborating
    operating-system microphone-use signal from the same browser process tree, the
    detector shall not produce a determinate start decision.
  priority: must
- id: FR-010
  statement: While automatic mode is active and a determinate start decision is produced,
    the system shall arm a cancellable ten-second countdown before capture begins,
    and cancellation shall suppress re-arming for the same meeting identity until
    that identity's signals have been continuously absent for sixty seconds.
  priority: must
- id: FR-011
  statement: If neither the engine-owned operating-system notification channel nor
    an attached client declaring indicator and cancel capabilities can present the
    countdown, then the system shall not begin automatic capture and shall record
    the suppressed decision with its cause.
  priority: must
- id: FR-012
  statement: When a determinate end decision is produced, the system shall hold the
    session for a sixty-second hysteresis window during which a continuing signal
    returns the session to recording on the same tracks without creating a new session.
  priority: must
- id: FR-013
  statement: The capture engine shall write each track as fixed-duration chunks that
    become durable by atomic rename before the manifest records them, bounding audio
    loss on abrupt termination to the single in-progress chunk.
  priority: must
- id: FR-014
  statement: When the engine restarts and finds a session left in the recording state,
    it shall recover the session from durable chunks under the same session identifier,
    mark the interruption explicitly on the timeline, and finalize rather than silently
    resume.
  priority: must
- id: FR-015
  statement: The recording model shall express every chunk's position as a sample
    offset on its own track timeline, shall represent missing audio as explicit gap
    records, and shall not derive any timestamp from concatenation order.
  priority: must
- id: FR-016
  statement: When track consolidation completes, the system shall verify that the
    decoded output is sample-identical to the durable chunk sequence with recorded
    gaps rendered as silence before deleting any chunk.
  priority: must
- id: FR-017
  statement: The system shall assign time-ordered unique identifiers to meetings,
    sessions, tracks, chunks, artifacts, workflow steps and exports, and shall reproduce
    each identifier verbatim in database rows, filesystem path segments and export
    payloads.
  priority: must
- id: FR-018
  statement: When a workflow step whose step key is already recorded as succeeded
    is enqueued again, the workflow runtime shall return the recorded result without
    re-executing any side effect.
  priority: must
- id: FR-019
  statement: When a processor, processor version or processor configuration changes,
    the workflow runtime shall derive a new step identity and shall retain the previous
    result rather than overwriting it.
  priority: must
- id: FR-020
  statement: The processor contract shall pass only explicitly staged input files
    to external processors, shall invoke them with an argument vector built from a
    manifest-declared template, and shall never construct a shell command line or
    place a secret in process arguments.
  priority: must
- id: FR-021
  statement: When a processor runs, it shall report monotonically non-decreasing progress
    at least once per work item and shall observe cancellation within one work item
    and within five seconds.
  priority: must
- id: FR-022
  statement: If local transcription of a two-hour recording exceeds the real-time
    budget, then the system shall emit a budget warning and shall continue the step
    rather than failing it.
  priority: must
- id: FR-023
  statement: When an export is retried after a crash or network failure, the destination
    shall reconcile against the recorded remote identity or the external-identifier
    marker before creating any remote object, so that no duplicate remote object is
    created.
  priority: must
- id: FR-024
  statement: The artifact store shall address artifacts as a root identifier plus
    a relative path composed only of generated identifiers, so that relocating the
    configurable artifact root does not invalidate any stored reference.
  priority: must
- id: FR-025
  statement: If an update or adapter manifest fails Ed25519 verification, declares
    a manifest version not greater than the installed version, or declares an artifact
    digest that does not match the file on disk, then the system shall reject it before
    using any value it declares.
  priority: must
- id: FR-026
  statement: The Phase 0 change package, its ADRs and its persistent design documents
    shall conform to the repository documentation schemas, lint target placement and
    status transition whitelist.
  priority: must
- id: FR-027
  statement: While a session is in the candidate or arming state, the system shall
    write no audio sample under the artifact root and shall persist session metadata
    only, so that a meeting that is detected but never recorded leaves no audio on
    durable storage.
  priority: must
- id: FR-028
  statement: When an armed countdown is cancelled, expires without entering the recording
    state, or is abandoned by a restart, the system shall leave no chunk file and
    no audio byte under the artifact root for that session.
  priority: must
- id: FR-029
  statement: When a user deletes a meeting, the system shall immediately make it invisible
    to every view, cancel its in-flight workflow steps, and shall purge its artifact
    directory and its derived rows, retaining only a tombstone carrying the meeting
    identifier, its timestamps and the identifiers of the remote objects it exported.
  priority: must
- id: FR-030
  statement: When a processing step executes work that loads a native inference library
    or runs an external program, the system shall execute that work in a child process
    supervised by the engine, so that the work's abnormal termination terminates only
    that child process.
  priority: must
- id: NFR-001
  statement: Secrets shall exist only in the operating-system credential store and
    shall never appear in application files, databases, artifacts, logs or process
    arguments.
  priority: must
- id: NFR-002
  statement: Diagnostic output shall contain no meeting audio, transcript text, summary
    text, meeting title, participant name or full URL.
  priority: must
- id: NFR-003
  statement: No component on the detection, capture, workflow, processing or export
    path shall depend on a first-party backend service.
  priority: must
- id: NFR-004
  statement: Local transcription without a GPU shall complete a two-hour recording
    within two hours of wall-clock time, with overrun treated as a warning.
  priority: must
- id: NFR-005
  statement: Detection inputs shall carry no DOM structure, selector, control label,
    screen coordinate, accessibility path or full URL, enforced by the closed signal
    envelope schema.
  priority: must
- id: NFR-006
  statement: Every outbound send shall append a local audit record naming destination,
    host, artifact identifier, byte count and outcome, and every egress host shall
    appear in the egress inventory.
  priority: must
- id: NFR-007
  statement: The application database shall reside under the local application-data
    directory regardless of the configured artifact root.
  priority: must
- id: NFR-008
  statement: The boundary check, the schema conformance checks, the egress inventory
    check and the documentation conformance check shall run in continuous integration
    on every push and pull request in the portable tier and shall block merge on failure,
    and the Windows tier shall run every registered T2 verification and shall block
    Phase 0 completion on failure.
  priority: must
- id: NFR-009
  statement: No crate on the capture path shall depend on the workflow, processor
    or destination crates, and no crate other than the processor host binary shall
    link a native inference library, so that a processing failure cannot reach the
    recording path.
  priority: must
- id: NFR-010
  statement: The contract-core crates shall build and pass their T0 and T1 verifications
    on a non-Windows host, and every T2 verification declared by this plan shall be
    registered in the Windows tier manifest.
  priority: must
---

<!-- lifecycle is owned by change.md -->

# Requirements

Phase 0 turns PLAN.md's Phase 0 deliverable list into falsifiable contracts. The requirements below are what the contracts in `implementation.md` discharge and what the checks in `verification.md` falsify. Every requirement is reached by at least one implementation contract or acceptance criterion; the canonical mapping lives in the design plan copied into `design-plan/`.

## Scope
Repository structure and crate topology; module boundary rules and their automated enforcement; the meeting-session state model and automatic-recording modes; the signal and detector contracts; the recording and artifact model including deletion; the engine/interface process topology and control channel; the local store contract; the workflow, processor and destination contracts; the threat model and credential policy including the repository egress inventory; verification tiering; and the documentation materialization of every Phase 0 decision.

Out of scope: real Windows signal collection and real WASAPI capture (Phase 1); detection heuristics and the per-application validation matrix (Phase 1 and Phase 5); any transcription, diarization or summarization implementation (Phase 3); real Google Drive and Notion clients (Phase 4); the browser extension itself (Phase 1); default retention *values*, which PLAN section 8 scopes to before Phase 2 (the deletion *mechanism* is in scope); and macOS, video capture, real-time translation and participant bots, which are PLAN section 4 non-goals.

## Functional requirements (EARS)

- **FR-001** (must) The repository shall provide a cargo workspace whose crate graph declares the workflow core, service adapters, platform collectors, capture engine, and composition roots as separate crates with declared layer membership.
- **FR-002** (must) When the dependency-direction check runs and any crate outside a composition root depends directly, transitively, or through a feature gate on a service-adapter crate, the check shall exit non-zero and name the offending dependency path.
- **FR-003** (must) When a core-layer crate contains a supported-service identifier literal, the boundary check shall exit non-zero and name the file, line and literal.
- **FR-004** (must) While a session is in the recording state, if the UI process terminates, then the capture engine shall continue writing durable chunks and shall keep the session in the recording state.
- **FR-005** (must) When a client connects or reconnects to the capture engine, the engine shall return an authoritative session snapshot with a monotonically increasing event sequence number, and the client shall render that state rather than a locally inferred state.
- **FR-006** (must) If a client connecting to the engine control channel does not belong to the same operating-system user as the engine, then the engine shall refuse the connection before dispatching any method and shall record a security diagnostic containing no payload.
- **FR-007** (must) When the detector replays a recorded signal timeline with the same configuration and adapter-table version, it shall emit a byte-identical decision sequence across runs and processes, and every decision shall cite the signal identifiers it used.
- **FR-008** (must) Where no adapter matches the observed subject, or where a matched adapter's corroboration requirement is unmet, the detector shall return unknown or inconclusive respectively and the session shall fall back to manual control without arming.
- **FR-009** (must) When the browser extension reports a meeting tab without a corroborating operating-system microphone-use signal from the same browser process tree, the detector shall not produce a determinate start decision.
- **FR-010** (must) While automatic mode is active and a determinate start decision is produced, the system shall arm a cancellable ten-second countdown before capture begins, and cancellation shall suppress re-arming for the same meeting identity until that identity's signals have been continuously absent for sixty seconds.
- **FR-011** (must) If neither the engine-owned operating-system notification channel nor an attached client declaring indicator and cancel capabilities can present the countdown, then the system shall not begin automatic capture and shall record the suppressed decision with its cause.
- **FR-012** (must) When a determinate end decision is produced, the system shall hold the session for a sixty-second hysteresis window during which a continuing signal returns the session to recording on the same tracks without creating a new session.
- **FR-013** (must) The capture engine shall write each track as fixed-duration chunks that become durable by atomic rename before the manifest records them, bounding audio loss on abrupt termination to the single in-progress chunk.
- **FR-014** (must) When the engine restarts and finds a session left in the recording state, it shall recover the session from durable chunks under the same session identifier, mark the interruption explicitly on the timeline, and finalize rather than silently resume.
- **FR-015** (must) The recording model shall express every chunk's position as a sample offset on its own track timeline, shall represent missing audio as explicit gap records, and shall not derive any timestamp from concatenation order.
- **FR-016** (must) When track consolidation completes, the system shall verify that the decoded output is sample-identical to the durable chunk sequence with recorded gaps rendered as silence before deleting any chunk.
- **FR-017** (must) The system shall assign time-ordered unique identifiers to meetings, sessions, tracks, chunks, artifacts, workflow steps and exports, and shall reproduce each identifier verbatim in database rows, filesystem path segments and export payloads.
- **FR-018** (must) When a workflow step whose step key is already recorded as succeeded is enqueued again, the workflow runtime shall return the recorded result without re-executing any side effect.
- **FR-019** (must) When a processor, processor version or processor configuration changes, the workflow runtime shall derive a new step identity and shall retain the previous result rather than overwriting it.
- **FR-020** (must) The processor contract shall pass only explicitly staged input files to external processors, shall invoke them with an argument vector built from a manifest-declared template, and shall never construct a shell command line or place a secret in process arguments.
- **FR-021** (must) When a processor runs, it shall report monotonically non-decreasing progress at least once per work item and shall observe cancellation within one work item and within five seconds.
- **FR-022** (must) If local transcription of a two-hour recording exceeds the real-time budget, then the system shall emit a budget warning and shall continue the step rather than failing it.
- **FR-023** (must) When an export is retried after a crash or network failure, the destination shall reconcile against the recorded remote identity or the external-identifier marker before creating any remote object, so that no duplicate remote object is created.
- **FR-024** (must) The artifact store shall address artifacts as a root identifier plus a relative path composed only of generated identifiers, so that relocating the configurable artifact root does not invalidate any stored reference.
- **FR-025** (must) If an update or adapter manifest fails Ed25519 verification, declares a manifest version not greater than the installed version, or declares an artifact digest that does not match the file on disk, then the system shall reject it before using any value it declares.
- **FR-026** (must) The Phase 0 change package, its ADRs and its persistent design documents shall conform to the repository documentation schemas, lint target placement and status transition whitelist.
- **FR-027** (must) While a session is in the candidate or arming state, the system shall write no audio sample under the artifact root and shall persist session metadata only, so that a meeting that is detected but never recorded leaves no audio on durable storage.
- **FR-028** (must) When an armed countdown is cancelled, expires without entering the recording state, or is abandoned by a restart, the system shall leave no chunk file and no audio byte under the artifact root for that session.
- **FR-029** (must) When a user deletes a meeting, the system shall immediately make it invisible to every view, cancel its in-flight workflow steps, and shall purge its artifact directory and its derived rows, retaining only a tombstone carrying the meeting identifier, its timestamps and the identifiers of the remote objects it exported.
- **FR-030** (must) When a processing step executes work that loads a native inference library or runs an external program, the system shall execute that work in a child process supervised by the engine, so that the work's abnormal termination terminates only that child process.

## Non-functional requirements

- **NFR-001** (must) Secrets shall exist only in the operating-system credential store and shall never appear in application files, databases, artifacts, logs or process arguments.
- **NFR-002** (must) Diagnostic output shall contain no meeting audio, transcript text, summary text, meeting title, participant name or full URL.
- **NFR-003** (must) No component on the detection, capture, workflow, processing or export path shall depend on a first-party backend service.
- **NFR-004** (must) Local transcription without a GPU shall complete a two-hour recording within two hours of wall-clock time, with overrun treated as a warning.
- **NFR-005** (must) Detection inputs shall carry no DOM structure, selector, control label, screen coordinate, accessibility path or full URL, enforced by the closed signal envelope schema.
- **NFR-006** (must) Every outbound send shall append a local audit record naming destination, host, artifact identifier, byte count and outcome, and every egress host shall appear in the egress inventory.
- **NFR-007** (must) The application database shall reside under the local application-data directory regardless of the configured artifact root.
- **NFR-008** (must) The boundary check, the schema conformance checks, the egress inventory check and the documentation conformance check shall run in continuous integration on every push and pull request in the portable tier and shall block merge on failure, and the Windows tier shall run every registered T2 verification and shall block Phase 0 completion on failure.
- **NFR-009** (must) No crate on the capture path shall depend on the workflow, processor or destination crates, and no crate other than the processor host binary shall link a native inference library, so that a processing failure cannot reach the recording path.
- **NFR-010** (must) The contract-core crates shall build and pass their T0 and T1 verifications on a non-Windows host, and every T2 verification declared by this plan shall be registered in the Windows tier manifest.

## Invariant requirements

Four requirements are invariants rather than event responses. They are called out because an implementation
can satisfy every event-driven requirement above and still violate one of these, and because each is the
falsifiable form of a promise PLAN makes to the user.

| Requirement | Invariant | Why it is stated as one |
| --- | --- | --- |
| FR-027 | No audio sample exists under the artifact root while a session is in `candidate` or `arming` | PLAN section 7's "users can cancel before automatic recording starts" is a disk-observable claim, not only an interface claim; without this, an implementation that keeps a pre-roll buffer on disk passes every other test |
| FR-028 | A cancelled, expired or abandoned countdown leaves no chunk file for that session | The complement of FR-027 across restart: recovery must remove the empty session rather than leave a phantom meeting |
| NFR-009 | The capture-path crates never reach the workflow, processor or destination crates, and only the processor host links a native inference library | PLAN section 7's "processing failure never stops the recording path" is a graph and process property; as prose it has no failing case |
| NFR-010 | Every declared T2 verification is registered in a tier and the contract-core crates build on a non-Windows host | Without registration, a Windows-only test can exist and never run, and the exit criteria it supports pass by absence |

## Delta against the upstream decisions

The seven user decisions recorded in the recovery packet are adopted as stated. Where this change adds
something the decision leaves open, it is because an implementation would otherwise be free to change what a
user observes:

- The automatic-recording decision fixes a 10-second countdown and a 60-second hysteresis. This change adds
  the remaining numbers (60-second cancel quiet period, 30-second prompt window, one 300-second extension),
  a suspend-excluding clock with re-evaluation on resume, and the rule that the engine's own notification is
  the primary consent surface so that automatic recording works with no interface running. It also fixes
  where ask mode's Start lives — on that same engine notification — because the decision says "recording
  starts only on Start" without saying who shows it, and the browser application class defaults to ask while
  the interface is normally closed.
- The store and artifact decision fixes locations. This change adds the pinning of the database under local
  application data regardless of the artifact root, identifier-only path segments, the two-writer ownership
  table, and the deletion mechanism with its idempotent purge and tombstone.
- The audio decision fixes formats and the chunk interval. This change adds the durability ordering, the
  bounded loss window, and verify-before-delete consolidation.
- The adapter decision fixes which adapters come first. This change adds the contract they must satisfy,
  including execution in a per-job child process.
- The transcription-budget decision fixes at most one times real time with mandatory progress and
  cancellation. This change adds the numbers that make "no progress" falsifiable: a 30-second per-work-item
  budget and a 150-second stall timeout, after which the host child is killed and the step is retryable with
  its completed work items preserved.
- The distribution decision fixes signed manifests. This change adds rollback protection, key rotation and
  the verify-before-use ordering.

## Acceptance criteria

| id | criterion | requirements |
| --- | --- | --- |
| A-01 | cargo xtask boundary and cargo deny check run in CI, exit 0 on the clean workspace, and exit non-zero on the negative fixture listing exactly the three planted violations and none of the three planted decoys. | FR-002, FR-003, NFR-008 |
| A-02 | No crate outside a composition root depends directly, transitively or through a feature gate on any adapter crate, and no core crate contains a class-A service identifier token or a class-B process, package or host literal. | FR-001, FR-002, FR-003 |
| A-03 | Killing the UI during a synthetic recording leaves the session recording with chunks still appearing; killing and restarting the engine recovers the session under the same identifier with at most one in-progress chunk unaccounted for, within ten seconds for a two-hour two-track session. | FR-004, FR-013, FR-014, FR-017 |
| A-04 | With no client process running at all, an automatic start decision delivers an engine notification, arms a cancellable ten-second countdown and starts capture; with notification delivery refused and no client attached, the same fixture creates no chunk file and records a suppression with its cause. | FR-010, FR-011, FR-012 |
| A-05 | Replaying a recorded signal timeline produces byte-identical decisions across runs and processes, every decision cites at least one signal identifier, and an extension-authority signal alone never yields a determinate start. | FR-007, FR-008, FR-009, NFR-005 |
| A-06 | Chunks and gaps tile each track range without overlap, and transcript timestamps after a lost chunk retain their true session position. | FR-015, FR-013 |
| A-07 | Consolidated FLAC decodes sample-identically to the chunk sequence before any chunk is deleted, and a crash between verification and deletion re-runs idempotently. | FR-016 |
| A-08 | Every entity identifier is byte-identical across database row, filesystem path and export payload; a duplicate enqueue of a completed step performs no work; and an effect interrupted between its intended and applied ledger rows is resolved by lookup or by an explicit user decision rather than by a silent recreate. | FR-017, FR-018, FR-019, FR-023 |
| A-09 | A planted secret and planted meeting content appear in no file the application writes, including diagnostic bundles, panic output and parse-error messages, and in no child process argument vector. | NFR-001, NFR-002, FR-020 |
| A-10 | A tampered, downgraded, unknown-key or digest-mismatched manifest is rejected before any declared value is used, and the engine binary is not replaced while a session is non-terminal. | FR-025, NFR-003 |
| A-11 | Every ADR and design document validates against the repository documentation schemas at an indexed path and starts at proposed, every design invariant names a mechanical check where one exists, and the three required change members are non-empty. | FR-026 |
| A-12 | The egress inventory declares every host reachable from source or from a processor or destination manifest, every entry maps to a user account, a distribution host or the operating system, an added undeclared host fails the check by name, and no audited egress host is absent from the inventory. | NFR-003, NFR-006 |
| A-13 | Local transcription reports monotonic progress, observes cancellation within five seconds, keeps per-item cost non-growing across a 240-item run, and treats budget overrun as a warning that still allows the step to succeed. | FR-021, FR-022, NFR-004 |
| A-14 | The database resides under the local application-data directory, stores no absolute artifact path, rejects writes outside a connection role's table family, and migrates forward from every released version while refusing a newer schema with a typed error. | FR-024, NFR-007, FR-005 |
| A-15 | A planted feature-gated capture-path edge onto the workflow crate and a planted native-inference dependency on the engine each fail the boundary check by rule name, and aborting the processor host child mid-job during a synthetic recording leaves chunk cadence unchanged, the session recording, and the step retryable. | NFR-009, FR-030, FR-004 |
| A-16 | In ask mode and after a cancelled countdown the artifact root contains zero audio bytes for that session, the meeting directory holds no chunk file, and a restart leaves no phantom meeting in the library. | FR-027, FR-028 |
| A-17 | After deleting a meeting and running the purge, the meeting identifier appears nowhere under the artifact root and in no row outside the tombstone; a second purge run is a no-op; a purge killed mid-walk resumes to completion on restart; and the exported remote objects still exist and are listed from the tombstone. | FR-029, NFR-002 |
| A-18 | The portable tier builds and passes the contract-core crates on a non-Windows host, every T2 verification identifier in this plan is registered in the tier manifest, CI defines both required jobs, and adding an unregistered Windows-only T2 test fails the registration check. | NFR-010, NFR-008 |
| A-19 | A client connecting to the engine control channel as a different operating-system user is refused before any method is dispatched, the refusal diagnostic names the rejection code and carries no request payload, the constructed pipe and token-file security descriptors grant the owning user only, and a pipe pre-created by another process is detected rather than adopted. | FR-006, NFR-001, NFR-002 |

## Non-goals for this change

- Accepting the ADRs. Fifteen ADRs are created at `proposed`; the transition to `accepted` is an authority
  act reserved to the decision makers and is a gate before the implementation units start.
- Producing product behaviour. Phase 0 ships contracts, seams and checks. The only executable behaviour is
  what is needed to make an exit criterion fail when it is violated.
- Choosing default retention values, detection thresholds, the reference machine class, or the final local
  model quantisation. Each is either scoped to a later phase by PLAN or routed to a named spike.
