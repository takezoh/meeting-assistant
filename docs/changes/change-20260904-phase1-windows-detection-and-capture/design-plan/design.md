# Phase 1 — Windows detection and audio-capture PoC

Canonical design plan. `revision: r2` — `r1` integrated `draft-1` with the twenty-five `verdict: Y` findings of
`critique.json`; `r2` applies the one verify finding that required reintegration,
`5-minimality-decision-fold`, which folds the per-application loopback-requirement record out of the shared
adapter contract (see "Approach", fourth correction, and
`adr-phase1-per-application-loopback-requirement-record`). Every other verify check was approved and its
content is carried forward unchanged. Machine skeleton: `spine.yaml` in this directory; every anchor below has
exactly one spine entry and vice versa.

This plan contains **no open design choice**. Every choice the draft preserved as open is closed here by an
implementation contract or an ADR, including the three the draft carried as "open pending confirmation"
(echo/leak representation, per-application loopback-requirement location, windows-rs and GNU fidelity). What
remains is typed implementation discretion — private, single-unit, reversible, mechanically checked — and two
ADR **acceptance** questions that are an authority act, not a design gap, listed at the end.

<!-- anchor: goal -->
## Goal

Implement the headless, diagnostic-first Windows 11 prototype PLAN.md §6 Phase 1 describes: prove that Teams
Desktop, Slack Huddle, Zoom Desktop and Google Meet in Chrome (with and without the detection-only extension)
can be detected and recorded from operating-system process, package, audio-session and microphone facts plus
the extension's tab signal, with no DOM access, no UI scraping and no new closed-schema field.

The nine PLAN §6 Phase 1 exit criteria translate as follows.

| PLAN exit criterion | What makes it falsifiable here |
| --- | --- |
| microphone and meeting audio can be recorded | `contract-process-loopback-capture` with a typed three-outcome activation result and a format pin to 16 kHz mono, plus `A-01`'s Windows-tier record |
| Meet start and end from extension plus OS microphone use, without DOM access | `contract-meet-corroboration-required`: `decide()` joins the two signals on `process_tree_root_pid`, and a recorded cross-tree fixture must yield `Inconclusive` |
| two-hour recording completes without losing data | `contract-two-hour-durability`: a synthetic 240-chunk accounting test on the portable tier plus a real two-hour Windows record |
| start and end signals observable for all four targets | `contract-replayable-timeline-fixtures`: five committed, redacted, replayable fixtures |
| detection requires no UI scraping | `contract-closed-schema-discipline`: the `Payload` and `Subject` field sets are frozen by a test, and every fixture line validates against the envelope schema |
| browser contamination and platform limits documented | `contract-process-loopback-capture`'s `contamination_risk` values plus the per-application manual records |
| the recorded microphone matches the meeting application's device | `contract-mic-endpoint-follows-session`, with the endpoint crossing as an explicit argument rather than as a crate edge |
| echo conditions and their severity documented per application | `contract-echo-leak-measurement`: one fixed statistic, one window rule, one alignment basis, three outcomes |
| recorded timelines replay as detector fixtures | `contract-replayable-timeline-fixtures` and `contract-detector-diagnostics-explainability`, against committed decision sidecars |

<!-- anchor: scope -->
## Scope

**In scope**, from PLAN.md §6 Phase 1's deliverable list:

1. Process/package identity, audio-session and microphone-use Windows collectors in the existing
   `ma-signals-windows` scaffold, behind the existing `SignalSource` seam.
2. Process-specific (including process-tree) loopback `CaptureSource`, system-loopback fallback and an
   always-available manual path, behind the existing `CaptureSource` seam.
3. Microphone endpoint selection that follows the meeting application's own capture session.
4. Meeting-audio leakage (echo) measurement into the microphone track, per application.
5. A per-application record of whether process-tree loopback is required, measured rather than authored.
6. A diagnostic harness and composition root in the existing `ma-engine` crate that wires the above, owns live
   session start/stop/cancel, appends the timeline incrementally, and exposes the confirmation-label command.
7. Signal timelines persisted in the existing replayable JSONL fixture shape with `.labels.json` and
   `.decisions.json` sidecars.
8. A detection-only Chrome/Edge extension PoC reporting tab host and audible state over the existing localhost
   channel, provisioned by the harness.
9. The `process_tree_root_pid` join in `ma-detect` that `FR-111` and the accepted extension-channel ADR both
   require and that nothing implements today.
10. Repository policy: the capture-path-isolation source list, a multi-plan verification registry, and the
    manual-verification procedure and record family that the split between the CI Windows job and real-hardware
    observation requires.

**Out of scope**, named so no Phase 1 component silently absorbs it:

- The session state machine, countdown, hysteresis, mode resolution and consent UI (`ma-session`, Phase 2).
  Phase 1 must not arm or start capture outside its explicit `record` subcommand (`NFR-105`).
- The workflow runtime, processors and destinations (Phase 3–4).
- The store as the write path for anything beyond what the durability harness already exercises.
- The consent-surface notification platform: Phase 1 is headless and has no live indicator to platform-select.
- Endpoint provisioning for a store-installed extension: Phase 1 provisions an unpacked PoC extension and the
  shipped mechanism is a later phase's decision (`adr-20260904-extension-endpoint-provisioning-poc`).
- macOS and video capture (Phase 6).

<!-- anchor: approach -->
## Approach

Every Phase 1 deliverable is a new producer behind an already-accepted, already load-bearing Phase 0 seam
rather than new architecture: `SignalSource` for every collector, `CaptureSource` for every audio path, the
existing `TimelineHeader`/JSONL/`.labels.json` shape for replay, and the existing `ExtensionMessage` contract
for the extension. The alternative of inventing Phase-1-specific formats was rejected because
`fixtures/signal-timelines/` already holds five recorded scenarios in this shape with a golden decisions file,
and `adr-20260903-detector-signal-replay-contract` already assigns Phase 1 "the first envelope revision" of
that same corpus.

Two structurally different capture strategies were weighed: (a) process-specific loopback per application with
system-loopback fallback, versus (b) system loopback only with post-hoc source separation. PLAN §4 accepts
(b)'s contamination only as a *browser* fallback when the extension is absent, and `CaptureMode` already has a
`ProcessLoopback` variant with `contamination_risk = None`; (a) is therefore the only option that uses that
variant for its purpose, and (b) survives as the fallback arm of the same contract.

Four structural corrections distinguish this plan from the draft. Three were forced by a blocker the critique
witnessed against repository facts at `ddeda34`; the fourth is a removal the minimality audit forced afterwards.

**A composition root now exists.** The draft delegated wiring to "the composition root" four times, but
`crates/ma-engine/Cargo.toml` depends only on `ma-core-types`, `ma-session`, `ma-ipc`, `ma-secure` and
`ma-store`, and no unit listed a `ma-engine` file. The smallest legal owner is the crate that already exists at
the top layer: `xtask/src/boundary.rs` skips the edge check when `rank == top`, so `ma-engine` (L5) may depend
on every collector, the capture engine, the extension channel and the four adapter crates, while placing the
same wiring in `ma-signals-windows` would be an L3→L3 violation. `contract-diagnostic-session-harness` gives
that wiring an owner, and with it the live-session control flow, the incremental timeline append, the
confirmation-label entry point and the decisions sidecar that the draft left unowned.

**Two facts leave the signal envelope instead of entering it.** `Subject` is a closed four-variant union with
`additionalProperties: false` and a `Signal` carries exactly one `Subject`, so a microphone signal attributed
to a process cannot also name an endpoint; and a sixty-second cross-track echo statistic is not an observation
of one subject at one instant. Rather than an ADR-gated schema bump for facts no detector consumes, both the
microphone endpoint and the echo measurement are carried as capture-side data outside the envelope. This
deletes the draft's reinterpretation of the shared `Payload.level_dbfs` field and keeps `NFR-104` intact.

**Real-hardware observations become records, not commands.** The hosted `windows-latest` runner has no Teams,
Slack, Zoom or Chrome installation, no speaker and no microphone, and runs nightly. Six of the draft's T2
checks needed exactly those, and two more had `command: null`, which `verify.rs` cannot even deserialise. The
smallest mechanism that keeps them honest is a declared procedure plus a committed record, gated by a command
the hosted runner *can* run: `cargo xtask manual-record --id <id> --require pass`.

**A third fact stays out of a shared contract.** The draft, and this plan before its minimality audit, recorded
the per-application process-tree-loopback requirement twice: once in the Windows-tier comparison record that
measures it, and again as an additive `requires_process_tree_loopback` field on `adapter.toml` and `AdapterSpec`
— an L1 contract four L4 crates, the shared conformance suite and the composition root all read. Phase 1 gives
that field no behavioural consumer: no match rule reads it, `adapter_table_version` is deliberately not bumped
for it, and the only thing a test can assert about it is that it equals the record. The conductor, under
user-delegated authority, folded it (`minimality-decisions.json`, 2026-09-04): the record is the single home,
and the widening waits for the phase that adds a consumer. What the second copy was really buying — the
guarantee that the fact exists for *every* application — is kept by the procedure's required-observation keys,
which cost one field in a Phase 1 policy file rather than a field in a shared contract.

<!-- section: granularity-profile -->
## Granularity profile

`boundary-complete`. Nearly every deliverable is new code behind a fixed, already-tested seam; the boundary and
its verification are the hard part, and Phase 1 fills it in rather than designing an algorithm-heavy core. Two
interiors are algorithm-shaped and are written out anyway because getting either wrong silently corrupts a
recorded result: the echo-return-loss method (`contract-echo-leak-measurement`) and the corroboration join
(`contract-meet-corroboration-required`). Two units carry a private, fixture-verifiable interior choice
(`discretion-package-identity-probe`, `discretion-mic-endpoint-matching-heuristic`) rather than needing an
`algorithm-bound` treatment of the whole plan.

<!-- section: design-dimensions -->
## Design dimensions

| Dimension | Carried by |
| --- | --- |
| `signal_observation` | `contract-process-package-identity`, `contract-audio-session-mic-use` |
| `capture_seam` | `contract-process-loopback-capture`, `contract-mic-endpoint-follows-session`, `contract-two-hour-durability` |
| `capture_measurement` | `contract-echo-leak-measurement` |
| `control_flow` | `contract-diagnostic-session-harness` |
| `detection_semantics` | `contract-meet-corroboration-required`, `contract-detector-diagnostics-explainability` |
| `data_contract` | `contract-closed-schema-discipline` |
| `fixture_format` | `contract-replayable-timeline-fixtures` |
| `signal_transport` | `contract-extension-signal-delivery` |
| `security_boundary` | `contract-extension-trust-reversal-check` |
| `boundary_enforcement` | `contract-capture-path-isolation-scope`, `contract-windows-tier-verification-registration` |
| `operational_evidence` | `contract-manual-verification-record`, `contract-per-app-loopback-requirement-record` |

Deliberate exclusions: `concurrency` (Phase 1 runs one diagnostic session at a time under an explicit command;
the multi-session case belongs to the Phase 2 session runtime) and `resource_management` (the only bound that
matters, the 60-second chunk-writer queue, is Phase 0's and is unchanged).

<!-- section: accepted-adr-context -->
## Accepted ADR context

Fifteen ADRs are accepted in the landed tree. Six are load-bearing here and are carried in `spine.yaml`'s
`adrs` array; the others (`capture-engine-process-isolation`, `desktop-stack-and-ipc`,
`phase0-executable-contract-skeleton`, `local-store-and-artifact-layout`, `update-and-manifest-distribution`,
`workflow-runtime-process-topology`, `workflow-identity-and-idempotency`, `local-transcription-budget`,
`initial-processor-adapters`) are adjacent and untouched by this plan.

One accepted ADR is not merely cited but **implemented for the first time**:
`adr-20260903-extension-localhost-channel-trust` states normatively that "a determinate start additionally
requires an operating-system microphone signal whose subject process belongs to the same browser process
tree". `grep -rn process_tree_root_pid --include=*.rs` returns one hit — the field declaration at
`crates/ma-signal/src/envelope.rs:100`. `decide()` keys candidates by `adapter_id` alone, so today any
`chrome.exe` microphone use corroborates any Meet tab. `adr-20260904-detector-process-tree-corroboration-join`
records how that clause becomes code, and it supersedes nothing: it discharges an obligation the accepted ADR
already created.

# Requirements

- <!-- anchor: fr-101 --> **FR-101** (must) The Windows signal collector shall observe process lifecycle and
  package identity for Teams Desktop, Slack Huddle, Zoom Desktop and the browser processes hosting Google Meet,
  using only OS process- and package-enumeration APIs, and shall emit `ProcessStarted`, `ProcessStopped` and
  `PackageIdentityObserved` signals carrying `Subject::Process{pid, image_name, package_family_name}` with no
  window title, control label or DOM-derived text in any field.

- <!-- anchor: fr-102 --> **FR-102** (must) The Windows signal collector shall observe per-process audio-session
  lifecycle and microphone-capture state for the same four target applications and shall emit
  `AudioSessionCreated`, `AudioSessionDestroyed`, `MicCaptureStarted` and `MicCaptureStopped` signals attributed
  to the owning process, **from the session-manager notification source only**, within a one-second
  observation-latency bound measured on that primary source; the consent-store poll shall corroborate and shall
  never by itself cause a signal to be emitted. The draft's unqualified "1 s bound" was arithmetically
  unreachable, because the corroborating source is itself polled at 1 s.

- <!-- anchor: fr-103 --> **FR-103** (must) When a target application's process is capturing audio and
  process-specific (including process-tree) loopback activation succeeds, `ma-capture` shall provide a
  `CaptureSource` whose `TrackOrigin` records `capture_mode = ProcessLoopback`, `contamination_risk = None`,
  `sample_rate = 16000` and `channels = 1` for that application's meeting-audio track.

- <!-- anchor: fr-104 --> **FR-104** (must) If process-specific loopback activation is unavailable or fails for a
  target application, `ma-capture` shall fall back to system (default-endpoint) loopback capture, recording
  `capture_mode = SystemLoopback` and `contamination_risk = PossibleOtherApps` on the resulting track, and a
  manual-start capture path (Device-mode `CaptureSource` selected by explicit user action) shall remain
  available independent of the loopback outcome.

- <!-- anchor: fr-105 --> **FR-105** (must) When a target application has an active audio-capture session on a
  specific microphone endpoint, the system shall open that same endpoint for the recorded microphone track
  rather than the system default communications device, taking the endpoint identifier as an explicit input
  supplied by the composition root from the audio-session collector's observation rather than through the signal
  envelope, and shall re-evaluate the selection through the existing `SourceEvent::FormatChanged` /
  successor-track path when the meeting application's endpoint changes mid-session.

- <!-- anchor: fr-106 --> **FR-106** (must) While a target application is recording on a speaker (non-headphone)
  audio path, the system shall compute the echo return loss between the loopback track and the concurrently
  captured microphone track as the difference of their root-mean-square levels in dBFS over one qualifying
  sixty-second window, and shall record that value per application together with the window's position on both
  tracks, both level values and the session's alignment uncertainty, or shall record an explicit
  `no_qualifying_window` or `inconclusive_alignment` outcome instead of a number.

- <!-- anchor: fr-107 --> **FR-107** (must) The system shall record, per target application and independent of any
  single recording, whether process-tree loopback rather than single-process loopback is required to capture
  that application's meeting audio completely, and that recorded value shall be derived from a measured
  comparison of the two activation modes against the same application rather than authored by hand.

- <!-- anchor: fr-108 --> **FR-108** (must) Every Phase 1 diagnostic session's signal timeline shall be appended
  to durable storage as each signal is observed, using the existing `TimelineHeader`-plus-JSONL fixture shape
  already committed under `fixtures/signal-timelines/`, so that a session ended by stop, by cancel or by a crash
  retains the signals observed before that point, and each persisted fixture shall be replayable by the existing
  detector-replay test path without modification to that path.

- <!-- anchor: fr-109 --> **FR-109** (must) The system shall provide the person conducting a Phase 1 diagnostic
  session an explicit command that records a "was this a meeting?" confirmation for one or more time ranges of a
  captured timeline, persisted using the existing `<timeline>.labels.json` sidecar shape (`timeline`,
  `labels[{from_monotonic_ns, to_monotonic_ns, was_meeting, note}]`).

- <!-- anchor: fr-110 --> **FR-110** (must) The detection-only browser extension shall report the active tab's
  host and audible state for Google Meet tabs to the desktop application over the existing localhost extension
  channel, using only the fields already defined by `ExtensionMessage` and only the browser's tabs API from its
  background service-worker context, and shall obtain the listener's port and per-start token from an endpoint
  file that the diagnostic harness writes into the unpacked extension directory, never from a filesystem,
  `nativeMessaging` or broad-host permission.

- <!-- anchor: fr-111 --> **FR-111** (must) The detector shall treat an extension tab signal and an
  operating-system microphone-use signal as corroborating each other only when both carry
  `payload.process_tree_root_pid` and the two values are equal, and shall otherwise return `Inconclusive` with a
  rule identifier naming whether the join key was absent or mismatched, so that no determinate Google Meet start
  is produced from extension tab evidence alone or from microphone use in a different browser process tree.

- <!-- anchor: fr-112 --> **FR-112** (must) When a Phase 1 diagnostic session ends, the system shall write the
  detector's decision output for that session's timeline to a committed `<timeline>.decisions.json` sidecar
  citing the signal identifiers and adapter rule id of every decision, so that the diagnostics are inspectable
  per session without re-running the detector.

- <!-- anchor: fr-113 --> **FR-113** (must) A Phase 1 capture session against any of the four target applications
  shall complete a two-hour recording through the existing chunk-writer and manifest durability path without
  data loss, as measured on the Windows verification tier.

- <!-- anchor: fr-114 --> **FR-114** (must) The capture-path-isolation rule in `boundary.toml` shall name
  `ma-signals-windows` and `ma-ext-channel` as additional sources, alongside the existing `ma-core-types`,
  `ma-session` and `ma-capture`, so the mechanically enforced rule matches the `module-boundaries.md` INV-002
  wording that no capture-path crate reaches `ma-workflow`, `ma-processor`, `ma-destination`, `ma-store`, or any
  adapter crate.

- <!-- anchor: fr-115 --> **FR-115** (must) When a Phase 1 collector starts or restarts while a condition it
  observes is already true, it shall emit `CollectorStarted` as its first signal and shall set
  `payload.restart_resync` on the first signal it emits for that already-true condition, so the detector's
  existing `resync-no-autostart` rule downgrades it instead of treating a collector restart as a fresh meeting
  start.

- <!-- anchor: fr-116 --> **FR-116** (must) The verification registry shall declare every canonical plan whose
  verification identifiers it holds and shall treat the union of those plans' declared identifiers as the
  registered set, so that adding Phase 1's plan does not make any Phase 0 registration stale and no plan's
  identifier can be registered twice.

- <!-- anchor: nfr-101 --> **NFR-101** (must) Every Phase 1 verification that requires real Windows OS behavior, a
  real target application, or real Chrome/Edge extension policy shall be registered with `platform = windows` in
  `verification-tiers.toml` and shall run in the existing CI windows job; no such verification shall be treated
  as satisfied by a portable-tier run alone.

- <!-- anchor: nfr-102 --> **NFR-102** (must) Every Windows-only dependency shall be declared under
  `[target.'cfg(windows)'.dependencies]` against a single workspace-level pinned version, every WASAPI or COM
  call site shall be behind `#[cfg(windows)]` with a portable fake backend behind the same trait, and
  `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` shall stay green on the
  ubuntu portable job after Phase 1 lands.

- <!-- anchor: nfr-103 --> **NFR-103** (must) The endpoint descriptor writer shall apply the owner-only security
  descriptor it already builds to `endpoint.json` before the file is used, and Phase 1 shall add Windows-tier
  verifications that observe (a) whether that applied DACL in fact prevents another same-user process from
  reading the token and (b) whether Chrome/Edge policy permits the detection-only extension to reach the
  loopback listener; the localhost-channel design in `adr-20260903-extension-localhost-channel-trust` shall not
  be superseded until both observations are recorded, and either observation showing a violation of the intended
  trust model shall be raised as a decision open for consultation rather than silently continuing.

- <!-- anchor: nfr-104 --> **NFR-104** (must) Every new Windows-observed datum shall be represented using an
  existing `SignalKind`, `Subject` or `Payload` field, or shall be carried outside the signal envelope entirely
  as capture-side or diagnostic data; introducing a new field or variant into the closed schema requires a
  `schema_version` bump proposed as an ADR, never an ad hoc addition.

- <!-- anchor: nfr-105 --> **NFR-105** (must) The Phase 1 diagnostic harness shall start capture only under an
  explicit operator-supplied subcommand, shall neither construct nor consult `ConsentSurfaces`, the countdown or
  the hysteresis path, and shall add no code path by which a detector decision alone can start a recording.

- <!-- anchor: nfr-106 --> **NFR-106** (must) Every Phase 1 verification whose observation cannot be made by an
  unattended hosted runner shall declare a manual verification procedure with a named owner, host profile,
  steps, artifact, pass criterion and the set of observations its record must carry, and its registered command
  shall fail when the corresponding record is absent, records an outcome other than `pass`, omits an observation
  the procedure declares as required, or was produced against a different revision of the procedure.

# Components

<!-- anchor: component-signals-windows -->
### component-signals-windows — Windows process, package, audio-session and microphone collectors

`crates/ma-signals-windows` is an existing L3 crate whose `Cargo.toml` has an empty `[dependencies]` table and
whose `src/lib.rs` is a one-line placeholder, already in the workspace member list and already assigned to L3
in `boundary.toml`. Phase 1 implements it behind the existing `SignalSource` seam and adds one non-`SignalSource`
accessor for the per-process capture endpoint. Owns `contract-process-package-identity` and
`contract-audio-session-mic-use`.

<!-- anchor: component-signal-contract -->
### component-signal-contract — signal envelope, collector seam, fixture shape (existing)

`crates/ma-signal` — the closed `Signal`/`SignalKind`/`Subject`/`Payload` schema, the `SignalSource` trait,
`TimelineHeader` and `SignalTimeline`. Phase 1 depends on all of it unmodified and owns keeping it unmodified
(`contract-closed-schema-discipline`) and keeping the fixture shape (`contract-replayable-timeline-fixtures`).

<!-- anchor: component-detector-core -->
### component-detector-core — pure detector, outcome partition, diagnostics (existing)

`crates/ma-detect` — `decide()`, the four-arm `Outcome` partition, `partition()`, and the generic-candidate
fallback. Unlike the draft, Phase 1 *does* change one thing here: the candidate-evaluation step gains the
`process_tree_root_pid` join. `decide()`'s signature, purity, the `Outcome` enum and `partition()` are
unchanged. Owns `contract-meet-corroboration-required` and `contract-detector-diagnostics-explainability`.

<!-- anchor: component-capture-engine -->
### component-capture-engine — CaptureSource seam, chunk writer, durability (existing)

`crates/ma-capture` — the `CaptureSource` trait whose doc comment reads "WASAPI arrives in Phase 1 behind this
trait", `SyntheticSource`, `chunk_writer.rs` with `SAMPLE_RATE = 16_000` and `CHUNK_SAMPLES = 480_000`,
`consolidate.rs` and `recovery.rs`. Phase 1 adds a `wasapi` module entirely behind the existing trait. Owns
`contract-process-loopback-capture`, `contract-mic-endpoint-follows-session`, `contract-echo-leak-measurement`
and `contract-two-hour-durability`.

<!-- anchor: component-engine-composition-root -->
### component-engine-composition-root — composition root and diagnostic harness (existing crate, new role)

`crates/ma-engine` is the existing L5 per-user background process crate. It is the smallest legal owner of
Phase 1's wiring: `xtask/src/boundary.rs` skips the layer check when `rank == top`, so L5 may depend on the
collectors, the capture engine, the extension channel and the four L4 adapter crates, all of which any lower
layer is forbidden to reach. Phase 1 adds the missing dependencies (with the adapter crates renamed in
`Cargo.toml` so no service identifier token appears in `ma-engine` source, which `boundary.toml`'s
`literals.allow_layers = ["L4"]` requires) and a second binary, `ma-diag`. Owns
`contract-diagnostic-session-harness`.

<!-- anchor: component-ext-channel -->
### component-ext-channel — detection-only browser channel server (existing)

`crates/ma-ext-channel` — `ExtensionMessage` with `deny_unknown_fields`, `Authenticator` (origin then token),
`FRESHNESS_WINDOW_MS = 5_000`, `MAX_MESSAGES_PER_SECOND = 20`, `MAX_QUEUED_SIGNALS = 200`, and
`EndpointDescriptor::write`, which today calls `std::fs::write` and returns a `SecurityDescriptor` nothing ever
applies. Phase 1 applies it, and adds one additive transport-supplied field to the internal `Request` type. The
wire schema, the auth logic, the limits and the status table are unchanged. Owns
`contract-extension-trust-reversal-check`.

<!-- anchor: component-extension-poc -->
### component-extension-poc — detection-only Chrome/Edge extension (new)

No `extension/` directory exists yet. A manifest-v3 background service worker speaking the existing
`ExtensionMessage` wire contract, with permissions exactly `["tabs"]` plus the loopback host, no content script
and no DOM access, per PLAN §4's non-goal. Owns `contract-extension-signal-delivery`.

<!-- anchor: component-fixture-corpus -->
### component-fixture-corpus — recorded signal-timeline fixtures (existing directory, new content)

`fixtures/signal-timelines/` holds five Phase 0 scenarios in the `TimelineHeader`/JSONL shape with
`.labels.json` sidecars and one `.decisions.json` golden. Every one uses `machine_profile: "redacted"` and
synthetic identifiers (`meet.example.test`, `example-browser.exe`); Phase 1's recordings follow that convention
rather than inventing one. Owns `contract-replayable-timeline-fixtures`.

<!-- anchor: component-boundary-policy -->
### component-boundary-policy — dependency-direction and literal rules (existing)

`boundary.toml` and `xtask/src/boundary.rs`. Phase 1 extends `[rules.capture-path-isolation].sources` only; no
new rule, forbidden target, literal class or layer assignment. Owns `contract-capture-path-isolation-scope`.

<!-- anchor: component-verification-registry -->
### component-verification-registry — verification-tiers.toml and its checker (existing)

`verification-tiers.toml` plus `xtask/src/verify.rs`. `check_registration` reads a single `file.plan` path and
reports every registration outside it as a stale registration, and `Registration.command` is a required
non-empty `String`. Phase 1 changes both facts: a `plans` array whose ids are unioned, and a `manual_record`
subcommand that makes a human observation registrable. Owns
`contract-windows-tier-verification-registration`.

<!-- anchor: component-manual-verification-record -->
### component-manual-verification-record — declared procedures and their records (new)

`manual-verification.toml` at the repository root, in the same pattern as `boundary.toml`,
`verification-tiers.toml` and `egress-inventory.toml`: a policy file with a conformance check. One procedure
per manual verification id, and one JSON record per performed observation under the change package. Owns
`contract-manual-verification-record` and, because the per-application loopback requirement is one of those
recorded observations rather than a declared table fact, `contract-per-app-loopback-requirement-record`.

The four `ma-adapter-*` crates have **no** component here, and that is deliberate: Phase 1 reads their tables
(the composition root links them, the fixture corpus points at their existing `fixtures.positive_process` and
`fixtures.positive_hosts` lists) but changes no file in them. The draft's `component-service-adapters` existed
only to own the adapter-table field this plan no longer adds.

# Implementation contracts

<!-- anchor: contract-process-package-identity -->
### contract-process-package-identity — process lifecycle and package identity, no UI text

**Owner** `component-signals-windows`. **Requirements** `FR-101`, `FR-115`. **Unit**
`windows-process-package-collector`.

Emits `ProcessStarted`, `ProcessStopped` and `PackageIdentityObserved` for the four target applications' processes
via OS process and package enumeration only, in the exact unchanged
`Subject::Process{pid, image_name, package_family_name}` shape. Every service identifier the collector needs is
constructor input supplied by the composition root from the `ma-adapter-*` tables; a literal in this crate fails
`cargo xtask boundary` immediately, because `literals.allow_layers` is `["L4"]` and this crate is L3.

*Operational inputs.* The image-name and package-family-name lists, owned by the four `ma-adapter-*` L4 crates
(existing repository structure this plan reads and does not modify, which is why they carry no component here),
produced by their `adapter.toml` tables, acquired at harness start through
`contract-diagnostic-session-harness`, valid for the life of the collector, and invalidated only by a harness
restart. A collector constructed with an empty list observes nothing and says so on `CollectorStarted`; it does
not fall back to a built-in list.

*Resync.* `CollectorStarted` is the collector's first signal. For any target process already running at start,
the first `ProcessStarted` carries `payload.restart_resync = true`. Neither field is new: both are declared in
`crates/ma-signal/src/envelope.rs` and `SignalKind::CollectorStarted` is one of the fifteen closed variants; the
draft named neither, which left the accepted `resync-no-autostart` rule inert because nothing ever set the flag.

*Failure semantics.* A package query failure and "never packaged" are distinct in the collector's internal
diagnostic and identical on the wire — both are `package_family_name = None`, because the closed schema has no
third state. That collapse is deliberate under `NFR-104`; the distinction is private and is delegated as
`discretion-package-identity-probe`, whose `escalate_when` fires exactly when the two become indistinguishable
or when the chosen API would force a boundary-confined literal.

*Witnesses.* Normal: four target processes running behind the fake enumerator yield the expected signals and no
other field (`v-win1-process-identity-fixture`). Adversarial, `security_boundary`: a decoy image name close to a
table entry (`Teams.exe.bak`) must not match the adapter table's `positive_process` fixture. Adversarial,
`lifecycle`: a process that exits between enumeration and the package query must yield `ProcessStarted` with
`package_family_name = None` and no panic, never a dropped signal.

<!-- anchor: contract-audio-session-mic-use -->
### contract-audio-session-mic-use — a two-source observation with one emitting source

**Owner** `component-signals-windows`. **Requirements** `FR-102`, `FR-115`. **Unit**
`windows-audio-session-mic-collector`. **ADR** `adr-phase1-windows-audio-signal-observation-apis`.

Emits `AudioSessionCreated`, `AudioSessionDestroyed`, `MicCaptureStarted` and `MicCaptureStopped` from
`IAudioSessionManager2` enumeration plus `IAudioSessionNotification`/`IAudioSessionEvents` state changes,
corroborated by `CapabilityAccessManager` consent-store timestamps polled at 1 s.

*Outcome partition.* The draft left this fallible two-source decision with no precedence rule, no epistemic
partition and no typed failure — unlike `contract-meet-corroboration-required`, which had one. The partition
recorded in `spine.yaml` is:

| outcome | condition | effect |
| --- | --- | --- |
| determinate | session-manager transition for a matched process | emit `MicCaptureStarted`/`MicCaptureStopped` within 1 s |
| unknown | neither source reports the process | emit nothing |
| inconclusive | consent-store usage window with no session-manager transition | emit nothing, increment the named inconclusive-source counter |
| conflicting | consent-store window still open while the session manager reports `Inactive`/`Expired` | session manager wins, emit `MicCaptureStopped`, count the conflict |
| failure `MicUseUnavailable` | notification registration fails for the collector's lifetime | report on `CollectorStarted`; never degrade to consent-store-only signals |

Coverage and exclusivity: the two sources give four combinations and the table names all four, plus the
registration failure that removes the primary source entirely. Precedence is total — the session manager decides
and the consent store only corroborates — so no implementation can differ on which source wins.

*Latency.* The 1 s bound is a property of the primary path alone. Corroboration lands within 2 s because the
secondary source is polled at 1 s; that number is recorded, not claimed as the bound. The bound is measured on
the Windows tier (`v-win1-mic-use-latency-live`); the portable tier exercises the same logic against a fake
session manager with synthetic timing, so the *logic* is tested everywhere and the *bound* only where it can be
truthfully measured.

*Process-tree root.* Every `MicCaptureStarted` and `MicCaptureStopped` for a browser process carries
`payload.process_tree_root_pid`, resolved from the process-tree lookup the collector already performs to
attribute the session. Without this the join in `contract-meet-corroboration-required` would compare `None` to
`None` on every signal.

*Endpoint observation.* The per-process capture endpoint is exposed through a non-`SignalSource` accessor, not
as a signal. `Subject` is closed with `additionalProperties: false`, a `Signal` carries one `Subject`, and
`Payload` has no endpoint field, so a `MicCaptureStarted` on `Subject::Process` cannot also name a device — and
no detector rule consumes an endpoint. See
`adr-20260904-mic-endpoint-observed-outside-the-signal-envelope`.

*Witnesses.* Normal: fake session-state transitions yield correctly attributed signals with the tree root set
(`v-win1-mic-use-fixture`). Adversarial, `epistemic`: a consent-store record with no session-manager transition
must emit nothing (`v-win1-mic-use-source-precedence`) — an implementation that emitted an `Os`-authority
`MicCaptureStarted` from the weaker source would let the detector raise a determinate start from evidence the
primary source never saw. Adversarial, `lifecycle`: a session already `Active` at collector start must produce
`restart_resync`.

<!-- anchor: contract-process-loopback-capture -->
### contract-process-loopback-capture — process loopback, its fallback, and a pinned format

**Owner** `component-capture-engine`. **Requirements** `FR-103`, `FR-104`, `FR-107`. **Unit**
`process-loopback-capture-source`. **ADRs** `adr-phase1-windows-audio-signal-observation-apis`,
`adr-20260903-audio-format-and-chunking`.

A `CaptureSource` implementation activating `ActivateAudioInterfaceAsync` with
`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, process-tree mode included, for a target application's process.

*Typed activation outcomes*, all legitimate and all observable, none a silent capture failure:
`Activated(ProcessLoopback, contamination_risk = None)` | `Fallback(SystemLoopback, PossibleOtherApps)` |
`ManualOnly(Device)`. Every outcome yields a valid `CaptureSource` that the durability path drives unchanged;
the three `CaptureMode` values and the two `ContaminationRisk` values already exist in `ma-core-types` and are
already persisted verbatim by the store schema, so this is a data-value change, not a schema change.

*Format pin.* `chunk_writer.rs:278` writes `origin.sample_rate` into the WAV header and `CHUNK_SAMPLES = 480_000`
means thirty seconds only at 16 kHz. A WASAPI device whose mix format is 48 kHz would silently shrink both the
chunk duration and the `QUEUE_CAP_SAMPLES` loss window to a third, and the draft's manifest-versus-directory
and no-data-loss checks both pass in that state. The source therefore resamples to 16 kHz mono before emitting
`SourceEvent::Samples` and reports `sample_rate = 16000, channels = 1` in its `TrackOrigin`; a backend that
cannot be resampled returns an activation error rather than opening a track whose origin rate differs from
`ma_capture::SAMPLE_RATE`. `v-win1-capture-origin-rate-pinned` is the discriminating check the draft lacked.

*FR-107's producer.* The per-application requirement is an empirical fact, so it has a measurement here rather
than an assertion in the adapter table: the Windows-tier procedure for `v-win1-loopback-live-activation`
captures the same meeting twice, once under single-process activation and once under process-tree activation,
and records per application whether the second captured audio the first missed. That record *is* FR-107's
per-application record; `contract-per-app-loopback-requirement-record` fixes what it must carry and what rejects
an incomplete one.

*Witnesses.* Normal: fake activation success yields `ProcessLoopback`/`None`; fake failure yields
`SystemLoopback`/`PossibleOtherApps` with a manual Device source still constructible in the same process.
Adversarial, `lifecycle`: activation denied mid-session after succeeding at start must surface as
`SourceEvent::FormatChanged` with a new origin through the existing successor-track path, never as silent
silence in the track. Adversarial, `data_loss`: a 48 kHz stereo mix format must not reach `chunk_writer`.

<!-- anchor: contract-mic-endpoint-follows-session -->
### contract-mic-endpoint-follows-session — the microphone the meeting application is using

**Owner** `component-capture-engine`. **Requirements** `FR-105`. **Unit** `mic-endpoint-follow-session`.
**ADR** `adr-20260904-mic-endpoint-observed-outside-the-signal-envelope`.

Opens the microphone `CaptureSource` on the endpoint the meeting application's own session is bound to, rather
than the system default communications device.

*The endpoint crosses as an argument, not as a crate edge.* `ma-capture` and `ma-signals-windows` are both L3
in `boundary.toml`, and `xtask/src/boundary.rs` allows an edge only when `dep_rank < rank` for a layer with no
`edges.restricted` entry, so an L3→L3 dependency is a violation, and recovery recorded that constraint as
`discovered-crate-layering-l0-l5` (`confirmed`). The draft's contract nonetheless required `ma-capture` to
consume the collector's per-process session data. Here the selection function takes
`preferred_endpoint_id: Option<&str>`; `contract-diagnostic-session-harness` reads the collector's
endpoint-observation accessor and passes the string. `ma-capture` names no type, trait or dependency from
`ma-signals-windows`, and routing the fact through `ma-signal` instead is unnecessary as well as impossible
under the closed `Subject` union.

*Endpoint change.* A change is delivered as a new hint and re-evaluated through
`TrackSegment::open_successor` and `SourceEvent::FormatChanged` — the mechanism Phase 0 added as a specific
review remediation (commit `ce4a808`). Phase 1 must not build a second device-change path beside it.

*Discretion.* When the hint changes more than once inside one selection window (a Bluetooth reconnect
mid-switch), which identifier is authoritative is private to
`crates/ma-capture/src/wasapi/mic_endpoint.rs`, reversible, and observationally identical under
`v-win1-mic-endpoint-fixture`; it is delegated as `discretion-mic-endpoint-matching-heuristic`, and its
`escalate_when` names the two conditions under which it stops being safely delegable.

*Witnesses.* Normal: a supplied non-default endpoint is opened; `None` opens the default and records that it
did. Adversarial, `lifecycle`: a mid-recording endpoint change opens a successor track whose origin names the
new endpoint. Adversarial, `boundary`: `cargo xtask boundary` must report no `ma-capture → ma-signals-windows`
edge.

<!-- anchor: contract-echo-leak-measurement -->
### contract-echo-leak-measurement — one statistic, one window, one alignment basis

**Owner** `component-capture-engine`. **Requirements** `FR-106`. **Unit** `echo-leak-measurement`.
**ADR** `adr-phase1-echo-leak-measurement-representation`.

The draft fixed neither the statistic, nor the window length, nor the unit semantics, nor the time base — while
`tracks_have_independent_origins` and `SessionTimeline.alignment_uncertainty_ms` exist precisely because sample
*n* of one track is not contemporaneous with sample *n* of another. Two conforming implementations would have
produced non-comparable per-application "severity" numbers that both pass the same fixture test. All four are
fixed here.

- **Statistic**: echo return loss in dB, `rms_dbfs(loopback over W) − rms_dbfs(microphone over W)`. A higher
  value means less leak.
- **Window `W`**: the first contiguous 60-second window in which the loopback track's 60-second RMS is at least
  −40 dBFS (the application really is producing audio) and no 20 ms frame of the microphone track exceeds
  −20 dBFS (no local speech).
- **Alignment basis**: `W` is located on each track by its own `TrackOrigin.start_monotonic_ns`. A 60-second
  energy comparison is insensitive to the tens of milliseconds `alignment_uncertainty_ms` can carry, which is
  why an energy ratio was chosen over a frame-wise correlation or a lag search; the recorded value carries the
  uncertainty so the reader can judge it.
- **Outcome partition**: `measured{erl_db, window_start_sample_per_track, loopback_rms_dbfs, mic_rms_dbfs,
  alignment_uncertainty_ms}` | `no_qualifying_window` | `inconclusive_alignment` (uncertainty above one second).
  A missing measurement is one of the last two, never a silent zero.
- **Storage**: the per-application measurement record, not the signal envelope. See the ADR for why
  `Payload.level_dbfs` is rejected.

*Witnesses.* Normal: paired fixture tracks with a synthesised 18 dB ERL yield 18 ± 1 dB with the window
position and both levels recorded. Adversarial, `epistemic`: a recording in which the local participant talks
throughout has no qualifying window and must record `no_qualifying_window`, not a number contaminated by
speech.

<!-- anchor: contract-two-hour-durability -->
### contract-two-hour-durability — two hours, 240 chunks, zero unexplained gaps

**Owner** `component-capture-engine`. **Requirements** `FR-113`. **Unit** `two-hour-durability-harness`.
**ADR** `adr-20260903-audio-format-and-chunking`.

Exercises the existing write-`.part` → flush → rename → manifest-append → fsync order against a two-hour
recording. The portable tier proves the accounting deterministically and without wall-clock time: a
`SyntheticSource` driven for 115 200 000 samples must yield exactly 240 chunks, a manifest naming exactly those
files, no gap record, and a total sample count equal to the produced count. The Windows tier records the real
two-hour run against a target application as a manual observation, because a two-hour attended recording is not
something the nightly hosted runner performs.

Note that this contract does **not** own the sample-rate assertion the draft attributed to it; that lives in
`contract-process-loopback-capture`, next to the source that can violate it.

<!-- anchor: contract-per-app-loopback-requirement-record -->
### contract-per-app-loopback-requirement-record — an application-level fact, measured and recorded

**Owner** `component-manual-verification-record`. **Requirements** `FR-107`. **Unit**
`verification-registry-multi-plan-and-manual-records`. **ADR**
`adr-phase1-per-application-loopback-requirement-record`.

The per-application requirement is written in exactly one place: the Windows-tier measured comparison record
`v-win1-loopback-requirement-live-comparison` commits, whose measurement `contract-process-loopback-capture`
owns. Phase 1 adds **no** field to `adapter.toml`, to `AdapterSpec` or to any other shared contract.

*Why not the adapter table, which the draft proposed.* `adapter.toml` and `AdapterSpec` are an L1 contract that
four L4 crates, the shared `conformance_violations()` suite and the composition root all read. Phase 1 gives
that field no behavioural consumer: no match rule reads it, `adapter_table_version` is deliberately not bumped
for it, and the only assertion available is that it equals the value the comparison record already states. A
second machine-readable copy of a recorded observation, inside a contract that wide, buys no observable outcome
the record does not already produce; it is deferred to the phase that adds a consumer — a capture path that
selects the activation mode from the declared value rather than probing at runtime.

*What keeps it falsifiable.* A committed record whose presence, outcome and freshness are gated but whose
*content* is unconstrained would let a green windows job stand on a record covering one application. So the
procedure declares the observation keys its record must carry — read from the adapter tables discovered under
`crates/ma-adapter-*/adapter.toml`, never written as literals, because `boundary.toml` confines service
identifiers to L4 and `xtask` is L5 — and `cargo xtask manual-record` rejects a record that omits one.
`v-win1-loopback-requirement-record-shape` is the portable check on that rejection and on the declaration's
completeness; it needs no real record, in the same way `v-win1-manual-record-staleness` needs none.

*Witnesses.* Normal: the procedure declares one required observation per adapter table, and a record carrying
all of them with a `pass` outcome makes `cargo xtask manual-record --id v-win1-loopback-requirement-live-comparison
--require pass` exit zero. Adversarial, `evidence`: a record that names three of the four applications, or that
records the requirement without the two activation modes' results it was derived from, must be rejected rather
than counted. Adversarial, `migration`: no committed decision id, fixture header or `adapter_table_version`
changes, because no adapter file is touched — the existing byte-identical replay of `desktop-start-end.jsonl`
is unaffected by this contract.

<!-- anchor: contract-replayable-timeline-fixtures -->
### contract-replayable-timeline-fixtures — real recordings, redacted, in the existing shape

**Owner** `component-fixture-corpus`. **Requirements** `FR-108`, `FR-109`. **Unit**
`signal-timeline-fixture-corpus`. **ADR** `adr-20260903-detector-signal-replay-contract`.

Five fixtures — Teams, Slack, Zoom, Meet with the extension and Meet without it — written in the exact
`TimelineHeader{schema_version, adapter_table_version, machine_profile, created}` plus one-signal-per-line JSONL
shape the five committed Phase 0 fixtures use, each with a `.labels.json` confirmation sidecar and a
`.decisions.json` decisions sidecar. This is not a new format decision: the accepted replay ADR already states
fixtures are "JSONL — a header record followed by one signal per line" and already assigns Phase 1 "the first
envelope revision" of this corpus.

*Redaction.* A recorded fixture would otherwise carry real pids, image names, package family names and tab
hosts into committed repository state, and `ma-detect` is L2 and may not reach an L4 adapter crate, so its
replay tests build their table inline from synthetic identifiers. Committed fixtures therefore keep
`machine_profile = "redacted"` and use synthetic pids, image names and hosts under a documented mapping, exactly
as `browser-tab-with-mic.jsonl` (`meet.example.test`, `example-browser.exe`) already does. The real identifiers
observed on the recording host are recorded in the Windows-tier manual record and are asserted where they are
allowed to live: the L4 adapter crates' own `fixtures.positive_process` and `fixtures.positive_hosts` lists.

*The decisions sidecar.* `FR-112` requires the diagnostics to be inspectable "without re-running the detector",
which a test that re-derives them at verification time does not satisfy. The harness writes each sidecar once
at session end; `v-win1-fixture-replay-golden` then asserts a fresh replay still equals the committed file, the
same relationship `desktop-start-end.decisions.json` already has.

*Witnesses.* Normal: each fixture replays byte-identically against its sidecar. Adversarial, `privacy`: a
fixture containing a real service identifier or a non-`"redacted"` machine profile must fail
`v-win1-fixture-redaction`. Adversarial, `schema`: a fixture whose header carries an extra field must fail
`v-win1-fixture-header-shape`.

<!-- anchor: contract-diagnostic-session-harness -->
### contract-diagnostic-session-harness — the composition root, and what a session does

**Owner** `component-engine-composition-root`. **Requirements** `FR-108`, `FR-109`, `FR-112`, `NFR-105`.
**Unit** `diagnostic-harness-composition-root`. **ADR** `adr-20260903-automatic-recording-modes`.

The draft named "the composition root" in four contracts and the "diagnostic harness" in four more, and neither
existed: `crates/ma-engine/Cargo.toml` depends on none of `ma-signal`, `ma-detect`, `ma-capture`,
`ma-signals-windows`, `ma-ext-channel` or any adapter crate, and no unit listed a `ma-engine` file. This
contract gives both an owner and states what they do.

*Wiring.* `ma-engine` adds those dependencies with the four adapter crates renamed in `Cargo.toml`
(`adapter_a = { package = "ma-adapter-teams" }`, and so on) so that no service-identifier token appears in
`ma-engine` source — `boundary.toml`'s class-A scan splits `ma_adapter_teams` into words and would match
`teams` outside L4. The harness reads the tables, passes their identifiers to the collectors, passes the
audio-session collector's observed endpoint into `ma-capture`'s selection call, and resolves the extension
listener's peer process to a process-tree root that the channel copies into tab signals.

*Control flow.* `SignalTimeline::merge` drains each source to exhaustion
(`while let Some(..) = source.next_signal()`), and its only implementation is `FixtureSource`, which is
exhaustible by construction; a live collector is not. The harness therefore uses `merge()` only on the offline
replay path. A live session is a loop that appends each observed signal to the session's JSONL file *before*
reading the next one, so that a session ended by `stop`, by `cancel` or by a crash keeps every signal observed
up to that point. `stop` and `cancel` differ only in whether the decisions sidecar is written; neither discards
the timeline.

*Entry points.* `ma-diag record` starts a session, `ma-diag label` attaches a `was_meeting` range to a
timeline's `.labels.json` sidecar, and `ma-diag replay` runs the offline path. `FR-109` says the system "shall
let the person record" the confirmation, which a hand-edited JSON file checked only for its shape does not
satisfy; the `label` subcommand is that entry point.

*No autostart.* Capture starts only under `record`. The harness constructs no `ConsentSurfaces`, no countdown
and no hysteresis state, and there is no path from a `decide()` outcome to a `CaptureSource`: the harness
records and the detector explains, and Phase 2 owns the connection between them. The already-registered
`v-consent-no-surface-no-start` (windows tier, `contract-consent-surface-precondition`, `ma-engine --test
consent no_surface_no_capture`) is left untouched; the draft redefined that same id against a different,
non-existent test, which would have made `check_registration` report one id twice with two meanings.

*Witnesses.* Normal: fake collectors drive a session whose JSONL grows signal by signal and whose sidecars are
written at end. Adversarial, `data_loss`: dropping the harness mid-session must leave every already-observed
signal on disk. Adversarial, `consent`: invoking `ma-diag` with no subcommand must construct no capture source
and write nothing under the artifact root.

<!-- anchor: contract-extension-signal-delivery -->
### contract-extension-signal-delivery — tab host and audible, over the existing channel

**Owner** `component-extension-poc`. **Requirements** `FR-110`. **Unit** `browser-extension-poc`. **ADRs**
`adr-20260904-extension-endpoint-provisioning-poc`, `adr-20260903-extension-localhost-channel-trust`.

The service worker reports `host` and `audible` for the active tab using only the fields `ExtensionMessage`
already defines; the server rejects any message with an extra field (`deny_unknown_fields`) or a non-hostname
`host`.

*How it learns the endpoint.* The listener binds an ephemeral port and the descriptor lives at
`%LOCALAPPDATA%\MeetingAssistant\ext\endpoint.json`, while an MV3 service worker limited to `tabs` and the
loopback host can read no file — so as drafted the extension could learn neither the port nor the token, and
`FR-110`/`A-02` were unsatisfiable. Phase 1 provisions the unpacked PoC extension from the harness, which writes
the current port and token into the extension directory it is given. This changes no accepted security rule:
the token still rotates per engine start, `Authenticator.check` still requires both a pinned
`chrome-extension://` origin and the token, and nothing new listens. See
`adr-20260904-extension-endpoint-provisioning-poc` for the three rejected alternatives.

*Permissions.* Exactly `["tabs"]` plus host permission `http://127.0.0.1/*` (Chrome host patterns carry no
port, so this covers the ephemeral port). No `content_scripts`, no `scripting`, no `nativeMessaging`, no
`storage`, no `<all_urls>`. `extension/` is outside the Cargo workspace `cargo xtask boundary` inspects, so this
is checked by a test in `ma-ext-channel` that reads `extension/manifest.json`, not by review.

*Witnesses.* Normal: a Meet tab yields a 204 with only the declared fields. Adversarial, `security_boundary`: a
manifest gaining any additional permission or a `content_scripts` key must fail
`v-win1-extension-manifest-permissions`. Adversarial, `lifecycle`: an engine restart rotates the token, so a
stale worker receives 401 and must stop and record rather than retry with a dead token.

<!-- anchor: contract-meet-corroboration-required -->
### contract-meet-corroboration-required — the same browser process tree, actually joined

**Owner** `component-detector-core`. **Requirements** `FR-111`. **Unit** `meet-process-tree-corroboration`.
**ADRs** `adr-20260904-detector-process-tree-corroboration-join`,
`adr-20260903-extension-localhost-channel-trust`.

The draft claimed this was "not new logic … already enforced twice". Neither cited enforcement compares process
trees: `conformance_violations()` rejects an adapter declaring `corroboration.tab` without
`corroboration.microphone`, and `partition(true, true, None)` yields `Determinate`. `decide()` sets
`candidate.microphone` from any OS `MicCaptureStarted` whose `Subject::Process` matches the adapter's
`browser_images`, and `candidate.tab` from any `TabMeetingPresent` on a matched host, with no tree check — so a
Meet tab in one Chrome window and an unrelated mic-using web call in a second Chrome process tree produce
`Determinate{Start}` for the Meet adapter, and Phase 2 would arm a recording of the wrong call. The draft then
forbade the fix in three places. The fencing is removed and the join is implemented.

*The rule.* For an adapter whose `Corroboration` requires both `tab` and `microphone`, the candidate carries the
`process_tree_root_pid` of each side and corroboration is met only when both are `Some` and equal:

| condition | outcome | `rule_id` |
| --- | --- | --- |
| both keys present and equal, no competing active meeting | `Determinate{Start}` | `start` |
| both keys present and equal, competing active meeting | `Conflicting{LowerPrecedence}` | existing |
| either key absent | `Inconclusive` | `process-tree-root-absent` |
| keys present and unequal | `Inconclusive` | `process-tree-mismatch` |
| no adapter matches the subject | `Unknown` | existing |

The `Outcome` enum, `partition()` and `decide()`'s signature and purity are unchanged; the join is a candidate
predicate, not a fifth arm. Desktop-class adapters require no tab evidence, so the rule does not apply to them
and `desktop-start-end.decisions.json` stays byte-identical — which the existing `replay_is_byte_identical`
test proves rather than asserts.

*Producers.* `browser-tab-with-mic.jsonl` already carries `process_tree_root_pid: 6300` on its tab signal and
pid `6300` on its mic signal, but the mic signal's payload omits the field; the unit adds it, which is why that
fixture appears in the unit's files. In production the collector sets it (`contract-audio-session-mic-use`) and
`ma-ext-channel`'s `signals_for` copies it from an additive transport-supplied `Request` field
(`contract-extension-trust-reversal-check`). Without both producers the join would compare `None` to `None`.

*Witnesses.* Normal: same-tree tab and mic corroborate. Adversarial, `security_boundary`: a mic signal from tree
`7100` against a tab in tree `6300` must yield `Inconclusive{process-tree-mismatch}`
(`v-win1-cross-tree-mic-does-not-corroborate`) — this is the case the forged-signal test does not cover, because
that test passes without any join. Adversarial, `epistemic`: a tab signal with no tree root must yield
`Inconclusive{process-tree-root-absent}`, never a determinate start by default.

<!-- anchor: contract-detector-diagnostics-explainability -->
### contract-detector-diagnostics-explainability — every decision cites its signals

**Owner** `component-detector-core`. **Requirements** `FR-112`. **Unit** `meet-process-tree-corroboration`.
**ADR** `adr-20260903-detector-signal-replay-contract`.

`Decision` already carries `rule_id` and `evidence: Vec<SignalId>`, and `Decision::derive` asserts the evidence
is non-empty. Phase 1's obligation is that this stays true against real Windows recordings including the two new
rule ids, and that the citation is readable from the committed sidecar without running anything.

<!-- anchor: contract-capture-path-isolation-scope -->
### contract-capture-path-isolation-scope — the enforced rule matches the stated invariant

**Owner** `component-boundary-policy`. **Requirements** `FR-114`. **Unit** `boundary-capture-path-scope`.
**ADR** `adr-20260903-workspace-boundary-enforcement`.

`[rules.capture-path-isolation].sources` lists only `ma-core-types`, `ma-session` and `ma-capture`, while
`module-boundaries.md` INV-002 reads over the whole capture path. Per the conductor's `align-config-to-doc`
disposition the rule is widened to include `ma-signals-windows` and `ma-ext-channel` rather than narrowing the
documented invariant. One thing is added beyond the disposition: an `xtask` test asserting the sources list
covers every crate the design documents call a capture-path crate, because re-running
`cargo xtask boundary --rule capture-path-isolation` is not discriminating — it passes vacuously when the list
is short, which is exactly the defect being fixed.

<!-- anchor: contract-windows-tier-verification-registration -->
### contract-windows-tier-verification-registration — more than one plan, and a portable build

**Owner** `component-verification-registry`. **Requirements** `FR-116`, `NFR-101`, `NFR-102`. **Unit**
`verification-registry-multi-plan-and-manual-records`. **ADRs**
`adr-20260904-verification-registry-multi-plan-and-manual-records`,
`adr-phase1-windows-rs-crate-and-gnu-fidelity`.

*Two plans.* `check_registration` reads a single `file.plan` path and reports "registered but not declared by
the plan (stale registration)" for anything outside it, and the reverse for anything declared and absent. Phase
1 is the repository's second plan, and nothing in the draft owned that. `verification-tiers.toml` gains
`plans = [...]`, `verify.rs` unions their declared ids, and the single-plan field keeps working. Repointing the
single field at Phase 1's spine was rejected: it would make all 112 Phase 0 registrations stale, including the
ids that `docs/design/*.md` invariants cite and that the `design-set` rule of `cargo xtask docs-check` requires
to stay registered.

*A portable build.* The portable CI job runs `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` on `ubuntu-latest`. Nothing in the draft fixed
`#[cfg(windows)]` gating for the new WASAPI and COM code, so two conforming implementations differed
observably: portable CI green versus red. `NFR-102` states the gating rule and
`v-win1-windows-code-is-cfg-gated` checks it mechanically by reading the two crates' manifests.

*What was deleted.* `v-win1-gnu-cross-check` is not registered. Its only evidence was
`discovered-linux-cross-compile-target-available`, recorded as `status: candidate` and scoped to one development
host; the `ubuntu-latest` job installs neither the `x86_64-pc-windows-gnu` target nor a mingw linker; and the
`windows-latest` runner's default toolchain is MSVC, so the check would merge-block on a promise it cannot keep
about a toolchain it does not test. Phase 0's own precedent applies: a rule the design violates on day one is
deleted rather than enforced. The real coverage is the Windows job compiling the crates.

<!-- anchor: contract-manual-verification-record -->
### contract-manual-verification-record — an observation a runner cannot make, gated by a check it can

**Owner** `component-manual-verification-record`. **Requirements** `NFR-106`. **Unit**
`verification-registry-multi-plan-and-manual-records`. **ADR**
`adr-20260904-verification-registry-multi-plan-and-manual-records`.

`.github/workflows/ci.yml` runs the windows job on a hosted `windows-latest` image with no Teams, Slack, Zoom or
Chrome installation, no speaker and no microphone, on a nightly and pull-request cadence. Nine Phase 1
verifications need exactly those, or two hours of wall clock, or a human judging a comparison;
`planning-source.txt` requires the split between the CI Windows job and a manual procedure explicitly. Two of
the draft's entries had `command: null`, which `verify.rs` cannot deserialise at all, and six more were bare
commands assumed to run unattended.

`manual-verification.toml` declares, per manual verification id: the owner, the host profile, the ordered steps,
the artifact path, the pass criterion and the observation keys the record must carry, plus the digest of the
procedure text. A performed observation is a committed JSON record naming the id, when and by whom it was
performed, the host profile, the outcome (`pass` | `fail` | `blocked`), the observations, and the procedure
digest it was performed against. `cargo xtask manual-record --id <id> --require pass` — the registered command,
which the hosted runner can run — fails when the record is absent, when the outcome is not `pass`, when a
declared required observation is missing, or when the digest differs, so editing a procedure invalidates every
record taken against the old one and a record cannot claim `pass` while leaving half the procedure's subject
unobserved. The required-observation keys are what `contract-per-app-loopback-requirement-record` uses to keep
FR-107's per-application record complete without a second copy of the fact in a shared contract.

*Why not a third tier.* Adding a `manual` tier would need `verify.rs`'s `plan_tier == "T2" → tier "windows"`
mapping to grow a third case and the CI job to grow a step. Registering the record check in the existing
windows tier costs one subcommand, keeps the exit gate exactly where PLAN puts it, and requires no tier logic
change.

*Witnesses.* Normal: a present, passing, current record makes the registered command exit zero. Adversarial,
`evidence`: an edited procedure with an unchanged record must fail (`v-win1-manual-record-staleness`).
Adversarial, `completeness`: a manual id with no procedure entry, or a procedure entry naming no plan-declared
id, must fail (`v-win1-manual-procedures-declared`).

<!-- anchor: contract-extension-trust-reversal-check -->
### contract-extension-trust-reversal-check — apply the ACL, then observe it

**Owner** `component-ext-channel`. **Requirements** `NFR-103`. **Unit**
`extension-endpoint-acl-and-trust-checks`. **ADR** `adr-20260903-extension-localhost-channel-trust`. The
owner-only descriptor this contract applies is the model `adr-20260903-threat-model-and-credential-policy`
fixes; that ADR is unchanged by this plan and is cited as authority rather than bound as a plan ADR.

`EndpointDescriptor::write` calls `std::fs::write` and returns a `SecurityDescriptor` it never applies;
`ma-secure/src/acl.rs` is a pure data structure whose own doc comment says applying it "is the platform unit's
job", and recovery recorded that Phase 0 "does not apply the ACL, only builds it"
(`discovered-ext-channel-endpoint-descriptor-path-acl`, `confirmed`). No draft unit applied it — yet
`NFR-103`(a) observes "whether `endpoint.json`'s owner-only DACL in fact prevents another same-user process from
reading the token" and binds the ADR supersede to that observation. Unapplied, the observation reports
"readable" mechanically and the supersede would be triggered by an unbuilt mechanism rather than by a real
finding.

Phase 1 therefore applies the descriptor the writer already builds, through an injected applier so the portable
tier can assert the call and the Windows implementation can set the file DACL from the descriptor's SDDL. The
descriptor's *shape*, `Authenticator.check`, the token lifetime, the rate, freshness and queue limits and the
rejection status table are all unchanged — which resolves the draft's self-contradiction, where the unit listed
`auth.rs` in its files while its guidance forbade touching it.

The same unit adds the additive `Request.peer_process_tree_root_pid` field that `signals_for` copies into
`Payload.process_tree_root_pid`. The association between a connection and its peer exists only inside the
server at handle time — `Server::drain()` returns bare `Signal`s with no connection id — so the composition root
cannot stamp it afterwards. The transport supplies the value; the peer-to-tree-root lookup itself is a Windows
call that lives in `ma-signals-windows` and is invoked by the L5 harness, so `ma-ext-channel` gains no
platform-specific dependency for it.

*Witnesses.* Normal: `write` invokes the applier with an owner-only descriptor. Adversarial, `security_boundary`:
a same-user process reading `endpoint.json` on a live Windows run is recorded as a fail with its evidence, not
skipped, and raises the ADR supersede as an open decision for consultation. Adversarial, `policy`: a browser
policy that blocks the loopback listener is recorded as a fail, not worked around.

<!-- anchor: contract-closed-schema-discipline -->
### contract-closed-schema-discipline — the field set is frozen, and the fixtures prove it

**Owner** `component-signal-contract`. **Requirements** `NFR-104`. **Unit** `closed-schema-fixture-guardrail`.

Phase 1 observes four new kinds of fact — package identity, microphone use, the microphone endpoint and the
echo measurement — and none of them adds a field. Two use existing fields (`package_family_name`,
`process_tree_root_pid`); two leave the envelope entirely as capture-side data. The guardrail is not a re-run of
Phase 0's tests: it validates every committed Phase 1 fixture line against
`contracts/signal/signal-envelope.schema.json` and asserts the exact `Payload` field set
(`restart_resync`, `audible`, `level_dbfs`, `command`, `calendar_event_key`, `process_tree_root_pid`) and the
exact four `Subject` variants, so an added field fails here before it reaches a fixture.

# ADRs

<!-- section: adr-identifiers -->
## ADR identifiers

Four of the decisions below answer decision points `draft-1/spine-draft.yaml` already carried, and they keep
that draft's identifier in this plan so the baseline correspondence stays exact. The repository's ADR schema
requires a document id of the form `adr-YYYYMMDD-slug` (`docs/schemas/_common.schema.json`) and
`cargo xtask docs-check`'s `adr-placement` rule requires the same shape as a filename, so a plan identifier of
the form `adr-phase1-...` cannot be a document id. Each is therefore materialised under the same slug with the
dated prefix the repository requires:

| plan identifier | repository document |
| --- | --- |
| `adr-phase1-windows-audio-signal-observation-apis` | `docs/adr/adr-20260904-windows-audio-signal-observation-apis.md` |
| `adr-phase1-echo-leak-measurement-representation` | `docs/adr/adr-20260904-echo-leak-measurement-representation.md` |
| `adr-phase1-per-application-loopback-requirement-record` | `docs/adr/adr-20260904-per-application-loopback-requirement-record.md` |
| `adr-phase1-windows-rs-crate-and-gnu-fidelity` | `docs/adr/adr-20260904-windows-rs-crate-and-gnu-fidelity.md` |

Three of those four close *against* the candidate the draft proposed — the echo measurement leaves the signal
envelope instead of reusing `Payload.level_dbfs`, the loopback requirement stays in the measured comparison
record instead of becoming an `adapter.toml` field, and the bindings decision is closed rather than deferred to
a spike — but the decision point is the draft's, so the identifier is too.

The four ADRs with no draft counterpart use their repository identifier in both places and each carries a
`scope_expansion_signal`, because a new recorded decision is structural growth over the draft baseline.
`adr-20260903-threat-model-and-credential-policy` is cited as authority in
`contract-extension-trust-reversal-check`'s prose but is deliberately **not** in `spine.yaml`'s `adrs` array:
this plan neither changes nor depends on a new part of it, and listing an untouched accepted ADR would make it
look like structure this phase added.

<!-- anchor: adr-phase1-windows-audio-signal-observation-apis -->
### adr-phase1-windows-audio-signal-observation-apis (proposed)

Bundles the two conductor-dispositioned technical choices into one decision because they are one subsystem:
`ActivateAudioInterfaceAsync` with `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` including process-tree mode
for capture, with per-application availability probed at runtime and a system-loopback fallback; and
`IAudioSessionManager2` plus `IAudioSessionNotification`/`IAudioSessionEvents` as the only emitting source for
microphone use, with `CapabilityAccessManager` consent-store timestamps polled at 1 s as corroboration only.
Binds `contract-process-loopback-capture` and `contract-audio-session-mic-use`; the outcome partition and the
precedence rule live in the latter.

<!-- anchor: adr-20260904-mic-endpoint-observed-outside-the-signal-envelope -->
### adr-20260904-mic-endpoint-observed-outside-the-signal-envelope (proposed)

The microphone endpoint a meeting application is using is capture configuration, not detection evidence, and is
exposed by the collector through a non-`SignalSource` accessor rather than through the closed envelope.
Alternatives rejected: an ADR-gated `Payload.endpoint_id` bump (a new closed-schema field for a fact no detector
rule reads, on a schema four crates and a conformance suite share) and a paired `Subject::Device` signal
correlated by time (no join key, and `SignalTimeline` orders by monotonic time only). Binds
`contract-mic-endpoint-follows-session` and `contract-audio-session-mic-use`.

<!-- anchor: adr-phase1-echo-leak-measurement-representation -->
### adr-phase1-echo-leak-measurement-representation (proposed)

Fixes the statistic (echo return loss in dB as a difference of RMS dBFS levels), the qualifying 60-second window
and its two admission conditions, the alignment basis (each track's own `TrackOrigin`, with
`alignment_uncertainty_ms` recorded), the three outcomes, and the storage location (a per-application record,
outside the signal envelope). Rejects the draft's default of reusing `Payload.level_dbfs`: a derived cross-track
statistic over a minute is not an observation of one subject at one instant, and reinterpreting a shared
closed-schema field would need the ADR-gated bump `NFR-104` demands anyway. Also rejects a dedicated
`leak_dbfs` field for the same reason. Binds `contract-echo-leak-measurement`.

<!-- anchor: adr-phase1-per-application-loopback-requirement-record -->
### adr-phase1-per-application-loopback-requirement-record (proposed)

The per-application process-tree-loopback requirement is recorded **only** in the Windows-tier measured
comparison record, with the procedure's required-observation keys making one entry per target application a
condition of the record being accepted. Alternatives rejected: the draft's additive
`requires_process_tree_loopback` field on `adapter.toml` parsed into `AdapterSpec` (a second machine-readable
copy of a recorded observation inside an L1 contract read by four L4 crates, the shared conformance suite and
the composition root, with no Phase 1 consumer — no match rule reads it, `adapter_table_version` is not bumped
for it, and its only assertion is equality with the record; deferred, explicitly, to the phase that adds a
behavioural consumer such as a capture path that selects the activation mode from the declared value), a new
`ma-store` table keyed by application (a writer role and a migration for policy data that is not a per-recording
observation), and a separate Phase 1 policy file at the repository root (a second home for per-service facts
with no rule for which file a new fact goes in). Binds `contract-per-app-loopback-requirement-record`.

<!-- anchor: adr-phase1-windows-rs-crate-and-gnu-fidelity -->
### adr-phase1-windows-rs-crate-and-gnu-fidelity (proposed)

One `windows` crate version pinned once in `[workspace.dependencies]` and used by every Windows-only crate;
every Windows dependency under `[target.'cfg(windows)'.dependencies]`; every WASAPI or COM call site behind
`#[cfg(windows)]` with a portable fake behind the same trait; and the `x86_64-pc-windows-gnu` cross-check
removed as a merge gate. This replaces the draft's spike-first deferral: the observable that matters is that
exactly one version is pinned and that the portable job stays green, neither of which varies with the numeric
version, so nothing here waits on a spike. Binds `contract-windows-tier-verification-registration` and
`contract-process-package-identity`.

<!-- anchor: adr-20260904-extension-endpoint-provisioning-poc -->
### adr-20260904-extension-endpoint-provisioning-poc (proposed)

The Phase 1 PoC extension is loaded unpacked and provisioned by the diagnostic harness, which writes the
current listener port and per-start token into a generated file in the extension directory that the service
worker imports at startup. Alternatives rejected: an origin-pinned bootstrap endpoint returning the token
(weakens the token against a same-user local process before `NFR-103`(a) has measured whether the token ever
protected against one, and adds a response body to a server whose responses are status-only); native messaging
now (the accepted ADR's own named reversal target — adopting it before Phase 1 collects the two observations
that ADR asks for would discard the evidence and the tested loopback suite); and installer or
`chrome.storage.managed` provisioning (Phase 1 has no installer, and a managed secret would have to outlive the
per-start token, contradicting the accepted ADR). Consequence recorded rather than hidden: this mechanism does
not generalise to a store-installed extension, and that provisioning decision belongs to the phase that builds
the installer. Binds `contract-extension-signal-delivery`.

<!-- anchor: adr-20260904-verification-registry-multi-plan-and-manual-records -->
### adr-20260904-verification-registry-multi-plan-and-manual-records (proposed)

`verification-tiers.toml` declares a `plans` array whose declared verification ids are unioned, keeping the
single-plan field as a one-element form; and a manual observation becomes registrable as a declared procedure
plus a committed, digest-pinned record gated by `cargo xtask manual-record`. Alternatives rejected: repointing
the single plan field (112 stale Phase 0 registrations and a broken `design-set` docs rule), copying Phase 0's
ids into Phase 1's spine (a plan declaring contracts it does not own), a third `manual` tier (tier-mapping and
CI changes for no additional guarantee), and leaving the checks as bare unattended commands (they would fail for
want of an installed application, or pass vacuously). Binds `contract-windows-tier-verification-registration`
and `contract-manual-verification-record`.

<!-- anchor: adr-20260904-detector-process-tree-corroboration-join -->
### adr-20260904-detector-process-tree-corroboration-join (proposed)

`decide()` joins a candidate's tab and microphone evidence on `payload.process_tree_root_pid`, the Windows
collector produces the value from the process-tree lookup it already performs, and `ma-ext-channel`'s
`signals_for` copies it from an additive transport-supplied `Request` field. Two new `rule_id` values,
`process-tree-root-absent` and `process-tree-mismatch`, make the two failure modes distinguishable in the
diagnostics. Alternatives rejected: leaving `ma-detect` fixed and asserting the property in the adapter tables
(a declarative table cannot compare two signals) and adding a new `SignalKind` carrying the pair (a new closed
variant for a fact two existing signals already carry). This ADR supersedes nothing: it discharges an
obligation `adr-20260903-extension-localhost-channel-trust` already created and that nothing implements. Binds
`contract-meet-corroboration-required`.

<!-- anchor: adr-20260903-detector-signal-replay-contract -->
### adr-20260903-detector-signal-replay-contract (accepted, carried forward)

Fixes the JSONL header-plus-signal-per-line shape, the `.labels.json` sidecar and the byte-identical replay
property, and explicitly assigns Phase 1 the first envelope revision of the corpus. Unchanged by this plan.

<!-- anchor: adr-20260903-extension-localhost-channel-trust -->
### adr-20260903-extension-localhost-channel-trust (accepted, carried forward and first implemented)

Fixes the loopback channel's auth, freshness, rate and queue constants, the non-authoritativeness of extension
evidence including the same-process-tree clause, and the two-condition reversal check. Phase 1 implements the
process-tree clause for the first time, applies the endpoint ACL it presumes, and records both reversal
observations. The ADR's decision is unchanged.

<!-- anchor: adr-20260903-workspace-boundary-enforcement -->
### adr-20260903-workspace-boundary-enforcement (accepted, carried forward)

Owns the `boundary.toml` mechanism whose `capture-path-isolation` sources list `FR-114` widens. The ADR's
decision is unchanged; only the rule's input list grows.

<!-- anchor: adr-20260903-automatic-recording-modes -->
### adr-20260903-automatic-recording-modes (accepted, carried forward)

Owns the consent-surface asymmetry and the memory-only pre-roll rule that `NFR-105` forbids Phase 1's harness
from bypassing. Unchanged by this plan.

<!-- anchor: adr-20260903-audio-format-and-chunking -->
### adr-20260903-audio-format-and-chunking (accepted, carried forward)

Fixes `SAMPLE_RATE = 16_000`, `CHUNK_SAMPLES = 480_000` and the durability write order that Phase 1's WASAPI
source must resample into rather than reinterpret. Unchanged by this plan.

<!-- section: unit-sequencing -->
# Unit sequencing

Fourteen units. The two collectors and `process-loopback-capture-source` have no dependencies and start in
parallel; `mic-endpoint-follow-session` and `echo-leak-measurement` follow the capture source;
`browser-extension-poc` is independent; `diagnostic-harness-composition-root` joins the collectors, the capture
selection and the extension and therefore follows all three; `signal-timeline-fixture-corpus` needs the harness
to record anything; `meet-process-tree-corroboration` needs the fixtures;
`closed-schema-fixture-guardrail` needs the fixtures; `two-hour-durability-harness`,
`extension-endpoint-acl-and-trust-checks`, `boundary-capture-path-scope` and
`verification-registry-multi-plan-and-manual-records` close the plan once their inputs exist — the last also
follows `process-loopback-capture-source`, because it declares the comparison procedure that source's
measurement fills. The draft's fifteenth unit, `per-app-loopback-requirement-record`, is gone: with no adapter
file to edit, the remaining work is one procedure declaration, one gate rule and one committed record, all of
them inside `verification-registry-multi-plan-and-manual-records`'s own files, and splitting it out would put
two workers in `manual-verification.toml` and `xtask/src/manual_record.rs`. No unit carries a
`decision_closure_reason`: the draft's two open representations are closed by ADRs here.

<!-- section: acceptance -->
# Acceptance

Fourteen criteria; full text and requirement refs are in `spine.yaml`'s `acceptance` array. `A-01`…`A-09` map
one-to-one onto PLAN §6 Phase 1's nine exit-criterion bullets. `A-10`…`A-14` cover the five obligations the
critique showed had no exit criterion: collector resync (`A-10`), registration and manual-record completeness
(`A-11`), the portable workspace build (`A-12`), the applied ACL and both trust observations (`A-13`), and the
no-autostart guarantee (`A-14`).

<!-- section: decision-input-reconciliation -->
# Decision input reconciliation

Fourteen decision inputs are carried and every one is dispositioned in `spine.yaml`. Nine come from the draft
and close one-to-one; five are new, each raised by a critique blocker that showed a material variation with no
owner: `decision-input-mic-endpoint-representation`, `decision-input-extension-endpoint-acquisition`,
`decision-input-verification-registry-multi-plan`, `decision-input-manual-vs-ci-verification-split` and
`decision-input-process-tree-join-owner`. None of the draft's inputs is dropped and none is closed by
restatement.

Three of the draft's inputs changed disposition rather than being carried forward as drafted, and each is a
strengthening, not a redefinition. `decision-input-echo-leak-measurement-representation` and
`decision-input-per-app-loopback-requirement-location` were `open` with a "documented default candidate"; both
are now closed by ADRs, and both close *against* that candidate — the echo measurement leaves the signal
envelope instead of reusing `Payload.level_dbfs`, and the loopback requirement stays in the measured comparison
record instead of becoming an adapter-table field.
`decision-input-windows-rs-crate-and-gnu-fidelity` was `open, evidence_seeking_decision, spike-first`; it is
now closed, because the observable that mattered turned out not to be the version number.

<!-- section: resolved-critique-findings -->
# Resolved critique findings

All twenty-five `verdict: Y` findings are resolved here, one row each, with the patch hint's disposition
recorded. The two `verdict: N` findings are left standing.

| finding | resolution | patch hint |
| --- | --- | --- |
| `issue-meet-process-tree-join-unimplemented` (blocker) | The "not new logic" claim and the three fencing statements are removed; `meet-process-tree-corroboration` implements the join in `decide()`'s candidate evaluation, with `process-tree-mismatch` and `process-tree-root-absent` as new rule ids, a new cross-tree fixture, and `adr-20260904-detector-process-tree-corroboration-join`. `contract-meet-corroboration-required`'s partition now states the join. | Adopted in full, and extended: the hint asked for the mismatch case; the absent-key case is added because a candidate whose key is `None` on either side would otherwise be `met` by default and reproduce the defect. |
| `issue-collector-restart-resync-unassigned` (blocker) | New `FR-115`; both collectors emit `CollectorStarted` first and set `payload.restart_resync` on the first signal for an already-true condition; `v-win1-collector-restart-resync-fixture` checks it. | Adopted in full, reusing the existing field and the accepted `resync-no-autostart` rule; extended from the mic collector to both, since the process collector has the same restart condition. |
| `issue-composition-root-and-harness-unowned` (blocker) | New `component-engine-composition-root` over the existing `crates/ma-engine`, new `contract-diagnostic-session-harness`, new unit `diagnostic-harness-composition-root` adding the missing dependencies (adapter crates renamed) and the `ma-diag` binary. | Adopted in full, including the hint's reasoning that L5 is the smallest legal owner because `boundary.rs` skips the check at `rank == top`. |
| `issue-mic-endpoint-requires-l3-to-l3-edge` (blocker) | `ma-capture`'s selection takes `preferred_endpoint_id: Option<&str>`; the harness reads the collector's accessor and passes the string; a witness asserts `cargo xtask boundary` reports no such edge. | Adopted in full. |
| `issue-portable-workspace-build-not-cfg-gated` (blocker) | `NFR-102` rewritten to state the gating rule; both Windows units gain the acceptance criterion; `v-win1-windows-code-is-cfg-gated` checks the manifests mechanically. | Adopted, and strengthened: the hint asked for guidance plus an acceptance criterion; a mechanical manifest check is added, because "clippy stayed green" does not distinguish a correctly gated crate from one that happens to compile today. |
| `issue-verification-registry-is-single-plan` (blocker) | `verification-tiers.toml` gains `plans = [...]`, `verify.rs` unions the declared ids, and `v-win1-registration-unions-plans` checks it. | Adopted in substance, not in form. The hint proposed repointing the single `plan` field at Phase 1's spine and carrying every Phase 0 id forward; that would either make 112 registrations stale or force Phase 1's plan to declare contracts it does not own, and it would break the `design-set` docs rule that requires every `v-*` id a design invariant cites to stay registered. Extending the mechanism is the smaller correct change and it *is* an `xtask` change, which the draft's `output_format` wrongly excluded. |
| `issue-extension-cannot-obtain-endpoint-token` (blocker) | Closed, not deferred: `adr-20260904-extension-endpoint-provisioning-poc` fixes harness provisioning of the unpacked PoC extension, with three named rejected alternatives; `FR-110` states it. | Adopted in substance, not in form. The hint proposed adding an open decision input and marking the unit blocked. A plan that leaves a blocker open is not closed, and the mechanism that changes no accepted security rule and adds no listener is available, so the choice is made here; the part that genuinely belongs to a later phase — provisioning for a store-installed extension — is recorded as the ADR's consequence, not as an open Phase 1 choice. |
| `issue-endpoint-json-acl-never-applied` (blocker) | `extension-endpoint-acl-and-trust-checks` applies the descriptor `EndpointDescriptor::write` already builds, through an injected applier so the portable tier can assert it; `NFR-103` states the obligation; `v-win1-endpoint-descriptor-acl-applied` checks it. | Adopted in full; the injected applier is added so the requirement has a portable check rather than only a Windows-tier observation. |
| `issue-t2-checks-need-a-host-the-ci-runner-is-not` (blocker) | New `NFR-106`, new `component-manual-verification-record`, new `contract-manual-verification-record`, `manual-verification.toml`, committed records and `cargo xtask manual-record`. Nine T2 ids are backed by records; `v-win1-endpoint-dacl-readability-observed` stays a real unattended Windows test because it needs only two same-user processes. | Adopted, with the mechanism made machine-checkable rather than documentary: the hint asked for "a documented manual-verification-procedure record" per id; a bare document would still pass silently, so the record is digest-pinned to its procedure and the registered command fails on absence, non-pass or staleness. |
| `issue-process-tree-root-pid-producer-unassigned` (major) | The collector populates the field for browser processes; `ma-ext-channel`'s `signals_for` copies it from an additive `Request.peer_process_tree_root_pid` the transport supplies. | Adopted, with the acquisition point corrected: the hint said `signals_for` populates it "from the connecting socket's peer process", but the server is transport-injected and `Request` carries no peer identity, and `Server::drain()` returns signals with no connection id, so the composition root cannot stamp it afterwards. The transport supplies the value on `Request`; the Windows peer lookup stays out of L3. |
| `issue-endpoint-process-binding-not-representable` (major) | Closed by `adr-20260904-mic-endpoint-observed-outside-the-signal-envelope`: the endpoint is capture-side data on a non-`SignalSource` accessor. | Adopted in substance, not in form. The hint asked for an open decision input following the plan's own open/documented-candidate pattern; the pattern itself is what this integration removes, and one closed answer that adds no closed-schema field is available, so the choice is made here with both alternatives recorded in the ADR. |
| `issue-null-command-verifications-cannot-register` (major) | Both entries now carry `cargo xtask manual-record --id … --require pass`, owned by `browser-extension-poc` and `extension-endpoint-acl-and-trust-checks` respectively. | Adopted in full. |
| `issue-decisions-sidecar-omitted` (major) | `.decisions.json` is in `signal-timeline-fixture-corpus`'s files, written once by the harness at session end (`v-win1-harness-decisions-sidecar`) and asserted against a fresh replay (`v-win1-fixture-replay-golden`), the same relationship `desktop-start-end.decisions.json` already has. | Adopted in full. |
| `issue-consent-verification-id-redefined` (major) | The already-registered `v-consent-no-surface-no-start` is left untouched; the harness guarantee is `v-win1-harness-requires-explicit-invocation` against a test that this plan creates. | Adopted, and generalised: the hint renamed one colliding id. The same defect was found in a second place — the draft reused `v-chunk-manifest-vs-directory`, which is already registered against `contract-chunk-durability` with the command `cargo test -p ma-capture directory_is_truth_manifest_is_cache` — so every Phase 1 id is now `v-win1-`-prefixed and no Phase 0 id is redefined. |
| `issue-echo-leak-semantics-and-time-base-unfixed` (major) | Closed by `adr-phase1-echo-leak-measurement-representation`: statistic, window, alignment basis, three outcomes and storage. | Adopted in substance, not in form. The hint asked to widen the existing open decision's scope; widening an open decision leaves it open. A 60-second energy ratio is chosen over a frame-wise statistic precisely because it is insensitive to `alignment_uncertainty_ms`, which turns the alignment problem into a recorded number rather than a methodology hazard. |
| `issue-sample-rate-drift-not-caught-by-its-own-verification` (major) | The source pins `origin.sample_rate = 16000, channels = 1` or fails activation; `v-win1-capture-origin-rate-pinned` is the check. The claim is moved out of `contract-two-hour-durability`, which cannot enforce it, into `contract-process-loopback-capture`, which can. | Adopted in substance, not in form: the hint asked to fold the assertion into the existing `v-chunk-manifest-vs-directory` test, but that id is already registered against a different Phase 0 contract with a different command, so a distinct `v-win1-` id is used and the obligation is attributed to the component that can violate it. |
| `issue-mic-use-two-source-partition-and-latency` (major) | `contract-audio-session-mic-use` gains the five-row outcome partition with total precedence and a typed `MicUseUnavailable` failure; `FR-102` restates the 1 s bound as a property of the primary path only. | Adopted in full, and extended with the registration-failure arm, since a partition over two sources that omits "the primary source is unavailable" still lets an implementation silently degrade to consent-store-only signals. |
| `issue-gnu-cross-check-rests-on-candidate-host-evidence` (major) | `v-win1-gnu-cross-check` is deleted rather than deferred; `NFR-102` states the gating and portable-green rule instead, and `adr-phase1-windows-rs-crate-and-gnu-fidelity` records why the cross-check is not a gate. | Adopted in substance, not in form: the hint made it conditional on a spike closing. Keeping a conditional registration keeps an open decision in the plan; deleting it and stating what the portable tier really proves closes it, and the Windows job compiling the crates is the coverage that was actually wanted. |
| `issue-loopback-requirement-observation-has-no-producer` (major) | `contract-process-loopback-capture` owns the measured comparison via `v-win1-loopback-requirement-live-comparison`, and the record that comparison commits *is* the per-application record FR-107 asks for; `contract-per-app-loopback-requirement-record` fixes what that record must carry and `v-win1-loopback-requirement-record-shape` checks that an incomplete one is rejected. | Adopted in substance, not in form. The hint's producer assignment stands; its consumer — a value on each adapter table — does not. The minimality fold (`minimality-decisions.json`, 2026-09-04) found no Phase 1 behaviour that reads such a value, so it would have been a second machine-readable copy of the record inside an L1 contract four L4 crates share. |
| `issue-confirmation-label-surface-unowned` (major) | The `ma-diag label` subcommand is the entry point, owned by `contract-diagnostic-session-harness` and checked by `v-win1-harness-label-command`. | Adopted in full. |
| `issue-live-timeline-merge-control-flow-unspecified` (major) | `contract-diagnostic-session-harness` fixes live start/stop/cancel and per-signal incremental append, and restricts `SignalTimeline::merge` to the offline replay path. | Adopted in full. |
| `issue-real-identifiers-and-host-data-enter-committed-fixtures` (major) | `contract-replayable-timeline-fixtures` requires `machine_profile = "redacted"` and synthetic identifiers under a documented mapping; `v-win1-fixture-redaction` checks it; the real identifiers are asserted in the L4 adapter crates' own fixture lists where `boundary.toml` permits them. | Adopted in full. |
| `issue-adapter-table-version-unaddressed` (minor) | The question no longer arises: no `adapter.toml`, `AdapterSpec` or `AdapterTable` file is touched, so `adapter_table_version`, the five committed fixture headers and every decision id in `desktop-start-end.decisions.json` are untouched, and `replay_is_byte_identical` remains the check that catches a real matching change rather than a bookkeeping one. | Adopted in substance, not in form. The hint asked to bump the version and regenerate the five fixtures and the golden; `Decision::derive` mixes the table version into every decision id, so that would have rewritten every committed id for a field that changes no decision. Removing the field is the smallest form of the same answer — nothing that could force a bump is added. |
| `issue-trust-reversal-unit-contradicts-itself` (minor) | The unit's guidance now forbids changing the descriptor's *shape*, `Authenticator.check` and the token lifetime while requiring the applier and the additive `Request` field, so `auth.rs` and `server.rs` are legitimately in its files. | Adopted in substance: the hint proposed removing `auth.rs` from the files list; since `issue-endpoint-json-acl-never-applied` puts the fix in `auth.rs`, the hint's own second branch applies and the guidance is corrected instead. |
| `issue-loopback-requirement-test-placement` (minor) | Neither placement survives: with no declared field there is nothing for an `ma-signal` test or an adapter crate's `tests/conformance.rs` to assert, and the one check is `v-win1-loopback-requirement-record-shape` in `xtask`, over the procedure's required-observation keys and the gate's rejection of a record that omits one. | Adopted in substance, not in form. The hint moved the assertion from `ma-signal` into the adapter crates' existing conformance seam; the fold moved the fact itself, so the check follows it to the artefact that now holds it. The hint's underlying point — do not invent a new test seam — is honoured: the check lands in the `xtask` test file the registry unit already owns. |
| `issue-mid-session-capture-mode-change-representation` (`verdict: N`) | Left standing. `TrackOrigin` is per `TrackSegment`, `open_successor` takes a new origin and `SourceEvent::FormatChanged` carries one, so a mid-session demotion is representable; it is written into `contract-process-loopback-capture`'s adversarial witness anyway. | No patch hint to apply. |
| `issue-package-identity-discretion-delegation` (`verdict: N`) | Left standing. `discretion-package-identity-probe` keeps its single private file, its `escalate_when` including the boundary-literal case, its `preserves` invariant and its `verification_refs`. | No patch hint to apply. |

<!-- section: scope-expansion-inventory -->
# Scope expansion inventory

Ten signals, one per expanded target and no target named twice, all recorded in `spine.yaml`'s
`scope_expansion_signals`. Every component and ADR in the final spine that has no counterpart in
`draft-1/spine-draft.yaml` appears here; everything else is baseline, carried at the draft's own identifier.

The eleventh signal this plan carried into verification, `scope-signal-adapter-table-additive-field`, is gone
with the structure it described. The conductor, under user-delegated authority, dispositioned it `fold`
(`minimality-decisions.json`, 2026-09-04): the adapter-table widening it accounted for is not made, so there is
no expansion left to declare and no surviving target to retarget it to. The four ADR signals audited alongside
it were dispositioned `necessary` and are unchanged.

**Group 1 — `critic_induced_contract`, the two new contracts.** `contract-diagnostic-session-harness` and
`contract-manual-verification-record` existed in no draft. Each exists because a critique blocker showed an
upstream-required observable with no owner, no requirement id and no check, and each traces to a PLAN §6
Phase 1 deliverable or to `planning-source.txt`'s explicit CI/manual split. The manual-verification record
family is one signal against its contract, covering both the contract and the operational artefacts it
introduces (`manual-verification.toml`, the committed records, the `manual-record` subcommand), because the
two are inseparable: the contract exists only to make a human observation registrable and the artefacts are
what it registers.

**Group 2 — `critic_induced_contract`, the two new components that own them.**
`component-engine-composition-root` and `component-manual-verification-record` are the owners those contracts
need. The first reuses the existing `crates/ma-engine` rather than adding a crate, and exists because
`xtask/src/boundary.rs` skips the layer check only at the top rank, so L5 is the sole legal place for the
wiring four contracts delegated to an unnamed "composition root". The second owns one policy file in the same
pattern as `boundary.toml` and `egress-inventory.toml`.

**Group 3 — four new ADRs, each closing a critique blocker the draft had no recorded decision for.** The
process-tree corroboration join, the microphone-endpoint representation, the extension endpoint provisioning
and the multi-plan registry with its manual-record gate. Each changes something an accepted decision or an
existing green check already depended on, so each is a recorded decision rather than an implementation detail;
the registry one is kinded `compatibility_operation` because its subject is how an existing check behaves
across two phases.

**Group 4 — `compatibility_operation` and `persistent_state`, on contracts.** The `plans` union exists because
pointing the single plan field at Phase 1's spine would strand 112 Phase 0 registrations, including the ids
`docs/design` invariants cite. Five recorded fixtures become committed repository state, bounded by the
redaction convention Phase 0 already established; that one is retained from the draft. No `shared_boundary`
signal remains: after the fold, Phase 1 widens no contract that another crate reads.

The four ADRs the draft already carried are **not** in this inventory, because they are baseline: they answer
the same decision points under the draft's own identifiers. See "ADR identifiers" below for how each is
materialised as a repository document.

Three of the draft's five signals are **not** retained, because their targets are gone. The reinterpretation of
the shared closed-schema `Payload.level_dbfs` field is deleted: the echo measurement leaves the signal envelope
entirely. The two fixture-format contracts are reduced to one: `contract-session-confirmation-label` is folded
into `contract-replayable-timeline-fixtures`, which now carries both `FR-108` and `FR-109`'s sidecar shape and
four verifications, while the *entry point* that records a label moves to the harness contract where its owner
is. And the adapter-table field is folded away, as above. The draft's fifth signal, the two-hour run on the
nightly windows gate, is retained but changed in kind: it is now a manual record, and the nightly gate runs the
record check rather than a two-hour recording.

<!-- section: assumptions -->
# Assumptions and constraints

1. Implementation and verification run on a Linux host; every contract pushes real WASAPI, COM, target-application
   and Chrome-policy behavior to the Windows tier or to a manual record, and maximises portable fixture and
   fake-backend coverage for everything else. Of the plan's 46 verifications, 36 are T0 or T1 on the portable
   tier.
2. Target platform is Windows 11 only; no Windows 10 compatibility path is designed or tested, which is what
   makes the process-loopback activation type safe to assume present.
3. The `windows-latest` GitHub-hosted runner has no Teams, Slack, Zoom or Chrome installation, no speaker and no
   microphone. This is the assumption `contract-manual-verification-record` exists to handle; if a
   self-hosted Windows runner with the four applications installed becomes available, nine manual records become
   ordinary registered commands and nothing else in the plan changes.
4. Phase 1 builds no session state machine, consent UI, workflow runtime or destination; `NFR-105` and
   `contract-diagnostic-session-harness` are the guardrail preventing the headless harness from quietly growing
   into an automatic-start path.
5. The eight proposed ADRs require acceptance by the named decision makers before implementation starts.
   Acceptance is an authority act reserved to those decision makers and is recorded as a `hard` governance gate
   in the change package, not as an open design choice here.

<!-- section: open-questions -->
# Questions for consultation

Neither is an open design choice: the plan is closed and buildable as written. Both are authority questions
whose answers would redirect a closed decision rather than fill a gap.

1. **Extension provisioning and the ADR's reversal condition.**
   `adr-20260904-extension-endpoint-provisioning-poc` keeps the accepted localhost channel and provisions the
   PoC extension from the harness, deliberately not pre-empting
   `adr-20260903-extension-localhost-channel-trust`'s reversal condition, which `NFR-103` measures in this same
   phase. If the intent is to move to native messaging now rather than collect those two observations first,
   that redirects `contract-extension-signal-delivery` and `contract-extension-trust-reversal-check` and should
   be decided before implementation starts, not after.
2. **The manual-verification obligation as a merge gate.** Nine PLAN §6 Phase 1 exit criteria become committed
   records that a person must produce on a real Windows machine with all four applications installed, and the
   nightly windows job fails until they exist. That is the honest reading of "browser audio contamination and
   other platform limitations are documented", but it makes Phase 1 completion depend on hardware access. The
   alternative — accepting a self-hosted Windows runner with the applications installed — removes the manual
   family for six of the nine and is a resourcing decision, not a design one.
