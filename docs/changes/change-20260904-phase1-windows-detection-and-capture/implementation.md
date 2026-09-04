---
change: change-20260904-phase1-windows-detection-and-capture
role: implementation
contracts:
- contract-process-package-identity
- contract-audio-session-mic-use
- contract-process-loopback-capture
- contract-mic-endpoint-follows-session
- contract-echo-leak-measurement
- contract-two-hour-durability
- contract-per-app-loopback-requirement-record
- contract-replayable-timeline-fixtures
- contract-diagnostic-session-harness
- contract-extension-signal-delivery
- contract-meet-corroboration-required
- contract-detector-diagnostics-explainability
- contract-capture-path-isolation-scope
- contract-windows-tier-verification-registration
- contract-manual-verification-record
- contract-extension-trust-reversal-check
- contract-closed-schema-discipline
contract_projections:
- id: contract-process-package-identity
  verifications:
  - v-win1-process-identity-fixture
  - v-win1-collector-restart-resync-fixture
  - v-win1-process-identity-live-probe
  discretion:
  - discretion-package-identity-probe
- id: contract-audio-session-mic-use
  verifications:
  - v-win1-mic-use-fixture
  - v-win1-mic-use-source-precedence
  - v-win1-mic-use-latency-live
  discretion: []
- id: contract-process-loopback-capture
  verifications:
  - v-win1-loopback-fallback-fixture
  - v-win1-manual-path-available
  - v-win1-capture-origin-rate-pinned
  - v-win1-loopback-live-activation
  discretion: []
- id: contract-mic-endpoint-follows-session
  verifications:
  - v-win1-mic-endpoint-fixture
  - v-win1-mic-endpoint-successor-track
  - v-win1-mic-endpoint-live
  discretion:
  - discretion-mic-endpoint-matching-heuristic
- id: contract-echo-leak-measurement
  verifications:
  - v-win1-leak-erl-fixture
  - v-win1-leak-no-qualifying-window
  - v-win1-leak-live-per-app
  discretion: []
- id: contract-two-hour-durability
  verifications:
  - v-win1-two-hour-chunk-accounting
  - v-win1-two-hour-live
  discretion: []
- id: contract-per-app-loopback-requirement-record
  verifications:
  - v-win1-loopback-requirement-record-shape
  - v-win1-loopback-requirement-live-comparison
  discretion: []
- id: contract-replayable-timeline-fixtures
  verifications:
  - v-win1-fixture-replay-golden
  - v-win1-fixture-header-shape
  - v-win1-fixture-redaction
  - v-win1-confirmation-label-sidecar-shape
  discretion: []
- id: contract-diagnostic-session-harness
  verifications:
  - v-win1-harness-requires-explicit-invocation
  - v-win1-harness-partial-timeline-survives-cancel
  - v-win1-harness-label-command
  - v-win1-harness-decisions-sidecar
  discretion: []
- id: contract-extension-signal-delivery
  verifications:
  - v-win1-extension-message-shape
  - v-win1-extension-manifest-permissions
  - v-win1-extension-live-chrome
  discretion: []
- id: contract-meet-corroboration-required
  verifications:
  - v-win1-same-tree-mic-corroborates
  - v-win1-cross-tree-mic-does-not-corroborate
  - v-win1-missing-tree-root-is-inconclusive
  discretion: []
- id: contract-detector-diagnostics-explainability
  verifications:
  - v-win1-diagnostics-cite-signals
  discretion: []
- id: contract-capture-path-isolation-scope
  verifications:
  - v-win1-capture-path-sources-cover-collectors
  discretion: []
- id: contract-windows-tier-verification-registration
  verifications:
  - v-win1-registration-unions-plans
  - v-win1-windows-code-is-cfg-gated
  - v-win1-portable-workspace-clippy
  discretion: []
- id: contract-manual-verification-record
  verifications:
  - v-win1-manual-procedures-declared
  - v-win1-manual-record-staleness
  discretion: []
- id: contract-extension-trust-reversal-check
  verifications:
  - v-win1-endpoint-descriptor-acl-applied
  - v-win1-endpoint-dacl-readability-observed
  - v-win1-browser-loopback-policy-observed
  discretion: []
- id: contract-closed-schema-discipline
  verifications:
  - v-win1-fixture-schema-conformance
  - v-win1-no-new-signal-fields
  discretion: []
adrs:
- adr-phase1-windows-audio-signal-observation-apis
- adr-20260904-mic-endpoint-observed-outside-the-signal-envelope
- adr-phase1-echo-leak-measurement-representation
- adr-phase1-per-application-loopback-requirement-record
- adr-phase1-windows-rs-crate-and-gnu-fidelity
- adr-20260904-extension-endpoint-provisioning-poc
- adr-20260904-verification-registry-multi-plan-and-manual-records
- adr-20260904-detector-process-tree-corroboration-join
- adr-20260903-detector-signal-replay-contract
- adr-20260903-extension-localhost-channel-trust
- adr-20260903-workspace-boundary-enforcement
- adr-20260903-automatic-recording-modes
- adr-20260903-audio-format-and-chunking
decision_dispositions:
- decision_input_ref: decision-input-wasapi-process-loopback-api
  disposition: 'adopted. assume (conductor, user-delegated authority, 2026-09-04):
    ActivateAudioInterfaceAsync with AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK
    including process-tree mode, per-application availability probed at runtime, system-loopback
    fallback with contamination_risk = PossibleOtherApps on activation failure. Comparison
    baseline system-loopback-only capture is rejected as the primary path because
    it leaves CaptureMode::ProcessLoopback unused for the three desktop applications
    it exists for. Fixed by adr-phase1-windows-audio-signal-observation-apis and contract-process-loopback-capture.'
  adr_refs:
  - adr-phase1-windows-audio-signal-observation-apis
  contract_refs:
  - contract-process-loopback-capture
- decision_input_ref: decision-input-microphone-use-observation-api
  disposition: 'adopted. assume (conductor, user-delegated authority, 2026-09-04):
    IAudioSessionManager2 enumeration plus IAudioSessionNotification/IAudioSessionEvents
    state changes as the only emitting source, CapabilityAccessManager consent-store
    timestamps polled at 1 s as corroboration only, one-second latency bound on the
    primary path. Comparison baseline polling IAudioSessionControl state without notifications
    is rejected because it gives no documented latency bound without busy polling.
    The two-source precedence, epistemic partition and typed startup failure are fixed
    by contract-audio-session-mic-use.'
  adr_refs:
  - adr-phase1-windows-audio-signal-observation-apis
  contract_refs:
  - contract-audio-session-mic-use
- decision_input_ref: decision-input-extension-trust-reversal-trigger-status
  disposition: 'verify-in-phase (conductor, user-delegated authority, 2026-09-04):
    the reversal condition of adr-20260903-extension-localhost-channel-trust is treated
    as not yet evaluated; Phase 1 applies the owner-only descriptor the writer already
    builds and then records both observations rather than pre-judging the outcome.
    A violation is raised as an open decision for consultation; the loopback channel
    stays the Phase 1 design until then.'
  adr_refs:
  - adr-20260903-extension-localhost-channel-trust
  contract_refs:
  - contract-extension-trust-reversal-check
- decision_input_ref: decision-input-capture-path-isolation-scope
  disposition: 'align-config-to-doc (conductor, user-delegated authority, 2026-09-04):
    boundary.toml''s capture-path-isolation sources are extended to include ma-signals-windows
    and ma-ext-channel so the enforced rule matches module-boundaries.md INV-002;
    the documented invariant is not narrowed. Comparison baseline narrowing the prose
    is rejected per the conductor''s rationale. Strengthened here by an xtask test
    that fails when the sources list stops covering every capture-path crate.'
  adr_refs:
  - adr-20260903-workspace-boundary-enforcement
  contract_refs:
  - contract-capture-path-isolation-scope
- decision_input_ref: decision-input-package-identity-observation-api
  disposition: implementation_detail. Delegated as discretion-package-identity-probe,
    a private, single-unit, reversible choice inside crates/ma-signals-windows/src/package_identity.rs
    whose alternatives all preserve package_family_name as Option<String> with None
    meaning "not packaged", verified by v-win1-process-identity-fixture. Escalates
    if packaged and non-packaged builds cannot be distinguished or if the chosen API
    forces a service-identifier literal outside an L4 crate.
  contract_refs:
  - contract-process-package-identity
- decision_input_ref: decision-input-engine-notification-platform
  disposition: not_applicable. Phase 1 is headless and diagnostic-first and builds
    no consent surface (NFR-105/contract-diagnostic-session-harness), so no Phase
    1 contract's observable varies with the choice of notification platform. It remains
    open for the phase that builds the real consent UI and is recorded there, not
    here.
  contract_refs: []
- decision_input_ref: decision-input-windows-rs-crate-and-gnu-fidelity
  disposition: 'adopted, replacing the drafted spike-first deferral. The observable
    that mattered is not which version is pinned but that exactly one version is pinned
    and that the portable job stays green: a single workspace-level [workspace.dependencies]
    pin used by every Windows-only crate, all Windows code behind a cfg(windows) attribute
    with a portable fake behind the same trait, and the x86_64-pc-windows-gnu cross-check
    removed as a merge gate because its only evidence was status=candidate on one
    development host and the ubuntu job installs neither the target nor a mingw linker.
    Fixed by adr-phase1-windows-rs-crate-and-gnu-fidelity and NFR-102; the numeric
    version is an implementation detail no observable depends on.'
  adr_refs:
  - adr-phase1-windows-rs-crate-and-gnu-fidelity
  contract_refs:
  - contract-windows-tier-verification-registration
- decision_input_ref: decision-input-echo-leak-measurement-representation
  disposition: 'adopted, closed against the drafted default. The Payload.level_dbfs
    reuse is rejected: a derived cross-track statistic over a sixty-second window
    is not an observation of one subject at one instant, and reinterpreting a closed-schema
    field read by four crates would need the ADR-gated bump NFR-104 demands anyway.
    A new dedicated Payload field is rejected for the same reason. The measurement
    leaves the signal envelope entirely and lives in a per-application capture-side
    record, and the statistic, window selection, alignment basis and outcome partition
    are fixed by adr-phase1-echo-leak-measurement-representation.'
  adr_refs:
  - adr-phase1-echo-leak-measurement-representation
  contract_refs:
  - contract-echo-leak-measurement
- decision_input_ref: decision-input-per-app-loopback-requirement-location
  disposition: 'adopted, closed against the drafted candidate. fold (conductor under
    user-delegated authority, 2026-09-04, minimality-decisions.json): the record lives
    in the Windows-tier measured comparison record the manual-verification family
    already commits, and Phase 1 adds no field to adapter.toml or AdapterSpec. Rejected
    alternatives: the drafted additive requires_process_tree_loopback field parsed
    into AdapterSpec (it widens an L1 contract four L4 crates, the shared conformance
    suite and the composition root read, and Phase 1 adds no behavioural consumer
    of it — no match rule reads it, adapter_table_version is deliberately not bumped,
    and its only asserted value is the one the comparison record already states),
    and a new ma-store table keyed by application (a writer role and a migration for
    policy data that is not a per-recording observation). What the fold would otherwise
    lose — that the record actually covers every target application — is kept by the
    required-observation keys the procedure declares, checked by v-win1-loopback-requirement-record-shape
    and enforced by the manual-record gate. The adapter-table field is deferred to
    the phase that adds a behavioural consumer for it. Fixed by adr-phase1-per-application-loopback-requirement-record.'
  adr_refs:
  - adr-phase1-per-application-loopback-requirement-record
  contract_refs:
  - contract-per-app-loopback-requirement-record
  - contract-manual-verification-record
- decision_input_ref: decision-input-mic-endpoint-representation
  disposition: 'adopted. Raised by critique issue-endpoint-process-binding-not-representable:
    Subject is a closed four-variant union with additionalProperties false and a Signal
    carries exactly one Subject, so a MicCaptureStarted attributed to Subject::Process
    cannot also name an endpoint, and Payload has no endpoint field. Rejected alternatives:
    an ADR-gated Payload.endpoint_id bump (a field no detector consumes) and a paired
    Subject::Device signal correlated by time (no join key). Adopted: the endpoint
    is capture-side configuration, not detection evidence, so the collector exposes
    it through a non-Signal accessor that the composition root passes into ma-capture
    as a string argument. Fixed by adr-20260904-mic-endpoint-observed-outside-the-signal-envelope.'
  adr_refs:
  - adr-20260904-mic-endpoint-observed-outside-the-signal-envelope
  contract_refs:
  - contract-mic-endpoint-follows-session
  - contract-audio-session-mic-use
- decision_input_ref: decision-input-extension-endpoint-acquisition
  disposition: 'adopted. Raised by critique issue-extension-cannot-obtain-endpoint-token:
    an MV3 service worker limited to the tabs API and the loopback host can read no
    file and therefore learns neither the ephemeral port nor the per-start token.
    Rejected alternatives: an origin-pinned bootstrap endpoint returning the token
    (weakens the token against a same-user local process before NFR-103(a) has measured
    whether that matters, and adds a response body to a server whose responses are
    status-only), native messaging now (the ADR''s own named reversal target, which
    would discard the evidence the ADR asks Phase 1 to collect), and installer or
    managed-storage provisioning (Phase 1 has no installer). Adopted: the diagnostic
    harness writes the current port and token into the unpacked extension directory
    it is given, which changes no accepted security rule. Fixed by adr-20260904-extension-endpoint-provisioning-poc;
    provisioning for a store-installed extension is explicitly a later phase''s decision.'
  adr_refs:
  - adr-20260904-extension-endpoint-provisioning-poc
  contract_refs:
  - contract-extension-signal-delivery
- decision_input_ref: decision-input-verification-registry-multi-plan
  disposition: 'adopted. Raised by critique issue-verification-registry-is-single-plan.
    Rejected alternatives: repointing the single plan field at Phase 1''s spine (makes
    112 Phase 0 registrations stale and breaks the docs design-set rule, which requires
    every v-* id cited by a design invariant to stay registered) and copying Phase
    0''s ids into the Phase 1 spine (a plan would then declare contracts it does not
    own). Adopted: a plans array whose declared ids are unioned, with the single-plan
    field still accepted. Fixed by adr-20260904-verification-registry-multi-plan-and-manual-records.'
  adr_refs:
  - adr-20260904-verification-registry-multi-plan-and-manual-records
  contract_refs:
  - contract-windows-tier-verification-registration
- decision_input_ref: decision-input-manual-vs-ci-verification-split
  disposition: 'adopted. Raised by planning-source.txt''s required split and by critique
    issue-t2-checks-need-a-host-the-ci-runner-is-not. Rejected alternatives: leaving
    the six checks registered as unattended commands on windows-latest (they would
    fail for want of an installed application or pass vacuously) and dropping them
    (they are PLAN section 6 Phase 1 exit criteria). Adopted: a declared procedure
    per manual id and a committed record, gated by cargo xtask manual-record, which
    the hosted runner can execute even though it cannot make the observation. Fixed
    by contract-manual-verification-record.'
  adr_refs:
  - adr-20260904-verification-registry-multi-plan-and-manual-records
  contract_refs:
  - contract-manual-verification-record
- decision_input_ref: decision-input-process-tree-join-owner
  disposition: 'adopted. Raised by critique issue-meet-process-tree-join-unimplemented
    and issue-process-tree-root-pid-producer-unassigned: FR-111 and the accepted extension-channel
    ADR both require the same-process-tree join, but process_tree_root_pid is read
    nowhere and produced nowhere. Rejected alternatives: leaving ma-detect fixed and
    asserting the property in the adapter tables (a table cannot compare two signals)
    and adding a new SignalKind carrying the pair (a new closed-schema variant for
    a fact two existing signals already carry). Adopted: decide() performs the join
    over the existing Payload field, the Windows collector populates it from the process-tree
    lookup it already performs, and the extension channel copies it from an additive
    transport-supplied Request field. Fixed by adr-20260904-detector-process-tree-corroboration-join.'
  adr_refs:
  - adr-20260904-detector-process-tree-corroboration-join
  contract_refs:
  - contract-meet-corroboration-required
  - contract-audio-session-mic-use
  - contract-extension-trust-reversal-check
milestones:
- id: signal-collectors
- id: capture
- id: composition-root
- id: detector
- id: fixtures
- id: extension
- id: security
- id: repository-policy
- id: guardrails
reference_algorithms: []
---

<!-- lifecycle is owned by change.md -->

# Implementation

Seventeen implementation contracts over eleven components, delivered as fourteen units. Every contract names an
owner component, the requirements it discharges, the seam it is built behind, its failure semantics and its
checks; the canonical machine-readable form is `design-plan/spine.yaml` in this package.

## Components and their layers

| component | crate or path | layer | new? |
| --- | --- | --- | --- |
| `component-signals-windows` | `crates/ma-signals-windows` | L3 | existing empty scaffold, implemented here |
| `component-signal-contract` | `crates/ma-signal`, `contracts/signal/` | L1 | existing, unmodified except tests |
| `component-detector-core` | `crates/ma-detect` | L2 | existing, one behaviour change |
| `component-capture-engine` | `crates/ma-capture` | L3 | existing, new `wasapi` module |
| `component-engine-composition-root` | `crates/ma-engine` | L5 | existing crate, new role |
| `component-ext-channel` | `crates/ma-ext-channel`, `contracts/extension-channel/` | L3 | existing, ACL apply and one additive field |
| `component-extension-poc` | `extension/` | outside the workspace | new |
| `component-fixture-corpus` | `fixtures/signal-timelines/` | — | existing directory, new content |
| `component-boundary-policy` | `boundary.toml`, `xtask/src/boundary.rs` | L5 | existing |
| `component-verification-registry` | `verification-tiers.toml`, `xtask/src/` | L5 | existing |
| `component-manual-verification-record` | `manual-verification.toml`, this package's `manual-verification/` | — | new |

Layer discipline is why `component-engine-composition-root` exists. `xtask/src/boundary.rs` computes
`allowed = dep_rank < rank` over the transitive reachability set, and skips the check entirely when
`rank == top`. `ma-capture` and `ma-signals-windows` are both L3, so neither may depend on the other, and both
are forbidden to reach an L4 adapter crate. `ma-engine` is L5 and may reach everything, so it is the only legal
place to link the collectors, the capture engine, the extension channel and the adapter tables together. Every
adapter dependency in `ma-engine/Cargo.toml` is renamed (`adapter_a = { package = "ma-adapter-teams" }` and so
on) because `boundary.toml`'s class-A literal scan splits `ma_adapter_teams` into words and `literals.allow_layers`
is `["L4"]`.

## Seams

Phase 1 adds no new seam. Every unit is a producer behind one that Phase 0 already ships and already tests.

| seam | defined in | Phase 1 implementations | portable fake |
| --- | --- | --- | --- |
| `SignalSource` | `ma-signal::source` | process/package collector, audio-session/mic collector | fake process enumerator, fake session manager, fake consent store |
| `CaptureSource` | `ma-capture::source` | process-loopback source, system-loopback fallback, manual Device source, microphone source | fake activation backend; the existing `SyntheticSource` |
| endpoint accessor | new, on `ma-signals-windows` | per-process capture endpoint, not a `Signal` | fake session endpoint |
| ACL applier | new, injected into `EndpointDescriptor::write` | Windows `SetNamedSecurityInfo`-backed applier | recording fake |
| activation backend | new, inside `ma-capture::wasapi` | `ActivateAudioInterfaceAsync` | fake that succeeds, fails, or reports unavailable |
| transport `Request` | `ma-ext-channel::server` | additive `peer_process_tree_root_pid` supplied by the transport | test constructs it directly |

Every WASAPI or COM call site sits behind a `cfg(windows)` attribute with the fake as the portable
implementation of the same trait, so `cargo test --workspace` and `cargo clippy --workspace --all-targets --
-D warnings` keep running the whole workspace on `ubuntu-latest`.

## Contracts

### Signal observation

**`contract-process-package-identity`** (`component-signals-windows`, FR-101, FR-115). Emits the three process
and package signals in the unchanged `Subject::Process` shape. Every service identifier is constructor input
supplied by the composition root from the adapter tables; a literal in this L3 crate fails `cargo xtask
boundary`. `CollectorStarted` is the first signal; an already-running target process yields
`restart_resync = true` on its first `ProcessStarted`. A failed package query and "never packaged" are distinct
internally and identical on the wire, because the closed schema has no third state; the exact call sequence
that keeps them distinct is delegated as `discretion-package-identity-probe`, private to
`crates/ma-signals-windows/src/package_identity.rs`, escalating if the two become indistinguishable or if the
chosen interface would force a boundary-confined literal.

**`contract-audio-session-mic-use`** (`component-signals-windows`, FR-102, FR-115). Session-manager
notifications are the only source that may emit; the consent store polled at one second corroborates only. The
outcome partition is total over both sources plus the registration failure:

| outcome | condition | effect |
| --- | --- | --- |
| determinate | session-state transition for a matched process | emit within one second |
| unknown | neither source reports the process | emit nothing |
| inconclusive | consent-store window, no session-manager transition | emit nothing, count it |
| conflicting | consent-store window open while the session manager says `Inactive`/`Expired` | session manager wins, emit `MicCaptureStopped`, count it |
| failure `MicUseUnavailable` | notification registration fails | report on `CollectorStarted`; never degrade to consent-store-only signals |

Every browser-process microphone signal carries `payload.process_tree_root_pid` from the process-tree lookup
the collector already performs to attribute the session; without it the detector's join compares `None` to
`None`. The per-process capture endpoint is exposed through a non-`SignalSource` accessor, not as a signal.

### Capture

**`contract-process-loopback-capture`** (`component-capture-engine`, FR-103, FR-104, FR-107). Three typed
activation outcomes — `Activated(ProcessLoopback, None)`, `Fallback(SystemLoopback, PossibleOtherApps)`,
`ManualOnly(Device)` — all legitimate, all observable, all yielding a `CaptureSource` the existing durability
path drives unchanged. The source resamples to 16 kHz mono before emitting samples and reports that origin;
a backend that cannot be resampled returns an activation error rather than opening a track whose origin rate
differs from `SAMPLE_RATE`, because `chunk_writer.rs` writes `origin.sample_rate` into the WAV header and
`CHUNK_SAMPLES` means thirty seconds only at 16 kHz. Mid-session activation loss surfaces as
`SourceEvent::FormatChanged` with a new origin, never as silent silence. The Windows-tier procedure for this
contract also produces FR-107's measured comparison between single-process and process-tree activation.

**`contract-mic-endpoint-follows-session`** (`component-capture-engine`, FR-105). Selection takes
`preferred_endpoint_id: Option<&str>`; the composition root reads the collector's accessor and passes the
string, so `ma-capture` names no type or dependency from `ma-signals-windows`. A change is re-evaluated through
`TrackSegment::open_successor` and `SourceEvent::FormatChanged`, the mechanism commit `ce4a808` added; Phase 1
does not build a second device-change path. Which identifier is authoritative when the hint changes twice
inside one selection window is delegated as `discretion-mic-endpoint-matching-heuristic`, private to
`crates/ma-capture/src/wasapi/mic_endpoint.rs`.

**`contract-echo-leak-measurement`** (`component-capture-engine`, FR-106). One statistic — echo return loss in
dB as `rms_dbfs(loopback) − rms_dbfs(microphone)` — over the first contiguous sixty-second window in which the
loopback track's RMS is at least −40 dBFS and no twenty-millisecond microphone frame exceeds −20 dBFS. The
window is located on each track by that track's own `TrackOrigin.start_monotonic_ns`; a sixty-second energy
comparison is insensitive to the alignment uncertainty the session timeline records, which is why an energy
ratio was chosen over a correlation. Three outcomes: `measured`, `no_qualifying_window`,
`inconclusive_alignment`. The value is a capture-side record, not a signal and not a `Payload` field.

**`contract-two-hour-durability`** (`component-capture-engine`, FR-113). The portable tier proves the
accounting deterministically: a `SyntheticSource` driven for 115 200 000 samples yields exactly 240 chunks, a
manifest naming exactly those files, no gap record, and a total equal to the produced count. The Windows tier
records the real two-hour run as a manual observation.

### Composition root

**`contract-diagnostic-session-harness`** (`component-engine-composition-root`, FR-108, FR-109, FR-112,
NFR-105). `ma-engine` gains the missing dependencies and a second binary, `ma-diag`, with three subcommands.

- `record` is the only path that constructs a `CaptureSource`. Invoked with no subcommand, the binary starts no
  collector, constructs no capture source and writes nothing under the artifact root.
- A live session appends each observed signal to the session's JSONL file *before* reading the next, so a
  session ended by `stop`, `cancel` or a crash keeps every signal observed up to that point. `stop` and
  `cancel` differ only in whether the decisions sidecar is written; neither discards the timeline.
  `SignalTimeline::merge` is used only on the offline replay path, because it drains each source to exhaustion
  and its only implementation, `FixtureSource`, is exhaustible by construction while a live collector is not.
- `label` attaches a `was_meeting` range to a timeline's `.labels.json` sidecar. This is FR-109's entry point;
  a hand-edited file checked only for its shape does not satisfy "the system shall let the person record".
- The harness reads the four adapter tables under renamed dependencies, passes their identifiers to the
  collectors, passes the collector's observed endpoint into `ma-capture`'s selection, and resolves the
  extension listener's peer process to a tree root that the channel copies into tab signals.
- There is no path from a `decide()` outcome to a `CaptureSource`, and no reference to `ConsentSurfaces`, the
  countdown or the hysteresis state. The already-registered `v-consent-no-surface-no-start` is left untouched.

### Detection

**`contract-meet-corroboration-required`** (`component-detector-core`, FR-111). For an adapter requiring both
tab and microphone corroboration, the candidate carries each side's `process_tree_root_pid` and corroboration
is met only when both are `Some` and equal.

| condition | outcome | rule id |
| --- | --- | --- |
| both present and equal, no competing active meeting | `Determinate{Start}` | `start` |
| both present and equal, competing active meeting | `Conflicting{LowerPrecedence}` | existing |
| either absent | `Inconclusive` | `process-tree-root-absent` |
| present and unequal | `Inconclusive` | `process-tree-mismatch` |
| no adapter matches | `Unknown` | existing |

`decide()`'s signature and purity, the `Outcome` enum and `partition()` are unchanged; the join is a candidate
predicate. Desktop adapters need no tab evidence, so the rule does not apply to them and
`desktop-start-end.decisions.json` stays byte-identical — which the existing `replay_is_byte_identical` test
proves rather than assumes. `browser-tab-with-mic.jsonl` already carries `process_tree_root_pid: 6300` on its
tab signal and pid 6300 on its mic signal; the unit adds the field to the mic signal's payload, and a new
`browser-tab-cross-tree.jsonl` carries 6300 against 7100.

**`contract-detector-diagnostics-explainability`** (`component-detector-core`, FR-112). `Decision` already
carries `rule_id` and non-empty `evidence`; Phase 1's obligation is that this stays true against real
recordings including the two new rule ids, and that the citation is readable from the committed sidecar.

### Data and fixtures

**`contract-replayable-timeline-fixtures`** (`component-fixture-corpus`, FR-108, FR-109). Five recordings —
Teams, Slack, Zoom, Meet with the extension, Meet without — in the exact committed header-plus-JSONL shape,
each with `.labels.json` and `.decisions.json` sidecars. Every fixture keeps `machine_profile = "redacted"` and
synthetic pids, image names and hosts under a documented mapping, exactly as the Phase 0 fixtures do; the real
identifiers observed on the recording host are recorded in the manual record and asserted where
`boundary.toml` permits them, in the L4 adapter crates' own fixture lists. The decisions sidecar is written once
by the harness and asserted against a fresh replay, the relationship `desktop-start-end.decisions.json` already
has.

**`contract-closed-schema-discipline`** (`component-signal-contract`, NFR-104). Validates every committed Phase
1 fixture line against `contracts/signal/signal-envelope.schema.json` and freezes the exact `Payload` field set
and the four `Subject` variants, so an added field fails before it reaches a fixture.

### Transport and security

**`contract-extension-signal-delivery`** (`component-extension-poc`, FR-110). Permissions are exactly `["tabs"]`
plus host permission `http://127.0.0.1/*`; no content script, no scripting, no native messaging, no storage, no
`<all_urls>`. The worker reads the endpoint file the harness writes into its directory, because a manifest-v3
service worker so limited can read neither the descriptor at `%LOCALAPPDATA%` nor the ephemeral port. A 401
after an engine restart stops the worker and is recorded rather than retried with a dead token. `extension/` is
outside the workspace `cargo xtask boundary` inspects, so the permission rule is checked by a test in
`ma-ext-channel` that reads the manifest.

**`contract-extension-trust-reversal-check`** (`component-ext-channel`, NFR-103). `EndpointDescriptor::write`
applies the owner-only descriptor it already builds, through an injected applier so the portable tier can
assert the call and the Windows implementation can set the file DACL from the descriptor's SDDL. The
descriptor's shape, `Authenticator::check`, the token lifetime, the rate, freshness and queue limits and the
rejection status table are unchanged. The same unit adds the additive `Request.peer_process_tree_root_pid`
that `signals_for` copies into `Payload.process_tree_root_pid`: the association between a connection and its
peer exists only inside the server at handle time, because `Server::drain()` returns bare `Signal`s with no
connection identifier, so the composition root cannot stamp it afterwards. The Windows peer-to-tree-root lookup
lives in `ma-signals-windows` and is invoked by the L5 harness, so `ma-ext-channel` gains no platform
dependency.

### Repository policy

**`contract-capture-path-isolation-scope`** (`component-boundary-policy`, FR-114). The rule's `sources` list
gains the two crates, and an `xtask` test asserts the list covers every capture-path crate — re-running
`cargo xtask boundary --rule capture-path-isolation` is not discriminating, because it passes vacuously when
the list is short, which is the defect being fixed.

**`contract-windows-tier-verification-registration`** (`component-verification-registry`, FR-116, NFR-101,
NFR-102). `verification-tiers.toml` gains `plans = [...]` whose declared identifiers are unioned, with the
single `plan` field still accepted; `verify.rs` changes accordingly. The `cfg(windows)` declaration rule is
checked mechanically by reading the two crates' manifests. `v-win1-gnu-cross-check` is not registered: its only
evidence was `status: candidate` on one development host, the ubuntu job installs neither the target nor a
mingw linker, and the shipping toolchain is MSVC.

**`contract-manual-verification-record`** (`component-manual-verification-record`, NFR-106).
`manual-verification.toml` declares one procedure per manual identifier with owner, host profile, ordered
steps, artifact path, pass criterion, the observation keys the record must carry, and the digest of the
procedure text; a performed observation is a committed JSON record naming the identifier, when and by whom, the
host profile, the outcome, the observations and the digest it was performed against. `cargo xtask manual-record
--id <id> --require pass` is the registered command, which the hosted runner can run; it fails on absence, on a
non-`pass` outcome, on a missing declared required observation, or on a digest mismatch.

**`contract-per-app-loopback-requirement-record`** (`component-manual-verification-record`, FR-107). The
requirement is written only in the Windows-tier measured comparison record
`v-win1-loopback-requirement-live-comparison` commits, whose measurement `contract-process-loopback-capture`
owns. No `adapter.toml`, `AdapterSpec` or `AdapterTable` file is touched, so `adapter_table_version`, the five
committed fixture headers and every decision identifier stay as they are. What keeps the record honest is not a
second copy of the fact but the procedure's declared required observations: one key per adapter table, read
from the tables discovered under `crates/ma-adapter-*/adapter.toml` rather than written as literals —
`boundary.toml` confines service identifiers to L4 and `xtask` is L5 — with `cargo xtask manual-record`
rejecting a record that omits one. `v-win1-loopback-requirement-record-shape` is the portable check on that
declaration and on the rejection, and it needs no real record, in the same way `v-win1-manual-record-staleness`
needs none. The adapter-table field the design draft proposed is deferred to the phase that gives the value a
behavioural consumer; `adr-phase1-per-application-loopback-requirement-record` records why.

## Units and order

Fourteen units. Dependency order is declared in `design-plan/spine.yaml`.

| # | unit | chunk | touches |
| --- | --- | --- | --- |
| 1 | `windows-process-package-collector` | signal-collectors | `crates/ma-signals-windows/src/{lib,process,package_identity}.rs`, its `Cargo.toml` |
| 2 | `windows-audio-session-mic-collector` | signal-collectors | `crates/ma-signals-windows/src/{audio_session,mic_use,endpoint_observation}.rs` |
| 3 | `process-loopback-capture-source` | capture | `crates/ma-capture/src/wasapi/{mod,process_loopback,manual_fallback}.rs`, its `Cargo.toml` |
| 4 | `mic-endpoint-follow-session` | capture | `crates/ma-capture/src/wasapi/mic_endpoint.rs` |
| 5 | `echo-leak-measurement` | capture | `crates/ma-capture/src/wasapi/leak_measure.rs` |
| 6 | `browser-extension-poc` | extension | `extension/{manifest.json,background.js,README.md}`, `crates/ma-ext-channel/tests/extension_poc.rs` |
| 7 | `diagnostic-harness-composition-root` | composition-root | `crates/ma-engine/{Cargo.toml,src/diagnostic/*,src/bin/ma-diag.rs}` |
| 8 | `signal-timeline-fixture-corpus` | fixtures | five JSONL fixtures with sidecars, `crates/ma-signal/tests/phase1_fixture_shape.rs` |
| 9 | `meet-process-tree-corroboration` | detector | `crates/ma-detect/src/detector.rs`, two fixtures |
| 10 | `closed-schema-fixture-guardrail` | guardrails | `crates/ma-signal/tests/phase1_schema_guard.rs` |
| 11 | `two-hour-durability-harness` | capture | `crates/ma-capture/tests/two_hour_durability.rs` |
| 12 | `extension-endpoint-acl-and-trust-checks` | security | `crates/ma-ext-channel/src/{auth,server}.rs`, `tests/trust_reversal.rs` |
| 13 | `boundary-capture-path-scope` | repository-policy | `boundary.toml`, `xtask/tests/boundary_policy.rs` |
| 14 | `verification-registry-multi-plan-and-manual-records` | repository-policy | `verification-tiers.toml`, `manual-verification.toml`, `xtask/src/{verify,manual_record,main}.rs`, `xtask/tests/registration.rs`, this package's records |

Units 1, 2, 3 and 6 have no dependency on each other and start in parallel. Units 4 and 5 follow 3. Unit 7
joins 2, 4 and 6. Unit 8 needs 7 to record anything; units 9 and 10 need 8. Units 11 through 14 close the plan
once their inputs exist. Unit 14 also follows unit 3, because it declares the comparison procedure that unit's
measurement fills, and it lands last because it registers every identifier the other thirteen introduce.

The design draft's fifteenth unit, `per-app-loopback-requirement-record`, is gone with the adapter-table field
it existed to add. What remains of `contract-per-app-loopback-requirement-record` is one procedure declaration,
one gate rule and one committed record, all inside unit 14's own files; splitting it out would put two workers
in `manual-verification.toml` and `xtask/src/manual_record.rs`. No unit touches a `crates/ma-adapter-*` file.

## Implementation discretion

Two typed delegations, each private to one file of one unit, reversible, and observationally identical under a
named portable check.

| id | unit | question | escalates when |
| --- | --- | --- | --- |
| `discretion-package-identity-probe` | 1 | which AppModel call sequence separates "not packaged" from "query failed" | the two become indistinguishable for an application shipping both builds, or the chosen interface forces a service literal outside L4 |
| `discretion-mic-endpoint-matching-heuristic` | 4 | which endpoint identifier is authoritative when the hint changes twice inside one selection window | the hint never settles before a format change fires, or two active target sessions supply different identifiers at once |

Nothing else is left to the implementer. The three questions the design draft carried as open — the echo
representation, the loopback-requirement location and the Windows bindings — are closed by ADRs, because each
of them would otherwise have let two conforming implementations produce a different observable result.
