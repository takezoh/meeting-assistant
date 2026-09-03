---
change: change-20260903-phase0-repository-and-contracts
role: implementation
contracts:
- contract-process-topology
- contract-processing-isolation
- contract-ipc-protocol
- contract-ipc-transport-authz
- contract-session-state-machine
- contract-recording-mode-policy
- contract-consent-surface-precondition
- contract-signal-envelope
- contract-detector-determinism
- contract-detector-outcome-partition
- contract-module-boundary-enforcement
- contract-extension-channel-trust
- contract-chunk-durability
- contract-session-timeline
- contract-track-consolidation
- contract-artifact-addressing
- contract-store-ownership
- contract-retention-purge
- contract-stable-identity
- contract-workflow-step-idempotency
- contract-processor-interface
- contract-processor-budget
- contract-destination-export-idempotency
- contract-credential-custody
- contract-diagnostic-redaction
- contract-egress-inventory
- contract-release-manifest-trust
- contract-docs-conformance
- contract-verification-tiering
contract_projections:
- id: contract-process-topology
  verifications:
  - v-topology-ui-kill
  - v-topology-single-instance
  - v-topology-engine-restart-resync
  - v-topology-update-deferred-during-session
  discretion: []
- id: contract-processing-isolation
  verifications:
  - v-isolation-capture-path-edges
  - v-isolation-native-link-confined
  - v-isolation-negative-fixture
  - v-isolation-processor-abort-keeps-recording
  discretion: []
- id: contract-ipc-protocol
  verifications:
  - v-ipc-schema-conformance
  - v-ipc-handshake-mismatch
  - v-ipc-resync-after-stall
  - v-ipc-backpressure-never-stalls-capture
  discretion:
  - discretion-jsonrpc-dispatch
- id: contract-ipc-transport-authz
  verifications:
  - v-authz-foreign-sid-rejected
  - v-authz-dacl-shape
  - v-authz-pipe-squat
  - v-authz-build-channel-carveout
  discretion: []
- id: contract-session-state-machine
  verifications:
  - v-session-table-conformance
  - v-session-exhaustive-step
  - v-session-idempotent-commands
  - v-session-crash-in-arming
  discretion:
  - discretion-state-machine-representation
- id: contract-recording-mode-policy
  verifications:
  - v-mode-resolution-order
  - v-mode-countdown-cancel-suppression
  - v-mode-suspend-resume-reevaluation
  - v-mode-hysteresis-flap
  discretion: []
- id: contract-consent-surface-precondition
  verifications:
  - v-consent-engine-notification-starts-without-client
  - v-consent-no-surface-no-start
  - v-consent-surface-loss-keeps-recording
  - v-consent-cancel-leaves-no-audio-byte
  discretion:
  - discretion-ui-state-store
- id: contract-signal-envelope
  verifications:
  - v-signal-schema-conformance
  - v-signal-no-ui-text-fields
  - v-signal-resync-no-autostart
  - v-signal-wall-clock-jump
  discretion: []
- id: contract-detector-determinism
  verifications:
  - v-detect-purity-lint
  - v-detect-replay-determinism
  - v-detect-evidence-present
  discretion: []
- id: contract-detector-outcome-partition
  verifications:
  - v-detect-partition-exhaustive
  - v-detect-conflict-precedence
  - v-detect-extension-alone-inconclusive
  - v-detect-adapter-panic-isolated
  discretion: []
- id: contract-module-boundary-enforcement
  verifications:
  - v-boundary-clean-workspace
  - v-boundary-negative-fixture
  - v-boundary-feature-gated-leak
  - v-boundary-ci-gate
  discretion:
  - discretion-boundary-check-graph-source
- id: contract-extension-channel-trust
  verifications:
  - v-ext-token-required
  - v-ext-origin-rejects-web
  - v-ext-replay-rejected
  - v-ext-alone-cannot-start
  discretion: []
- id: contract-chunk-durability
  verifications:
  - v-chunk-kill-recovery
  - v-chunk-manifest-vs-directory
  - v-chunk-backpressure-gap
  - v-chunk-2h-scale
  discretion:
  - discretion-chunk-writer-buffering
- id: contract-session-timeline
  verifications:
  - v-timeline-gap-preserving-timestamps
  - v-timeline-coverage-invariant
  - v-timeline-track-independence
  - v-timeline-format-change-segment
  discretion: []
- id: contract-track-consolidation
  verifications:
  - v-consolidate-lossless
  - v-consolidate-crash-idempotent
  - v-consolidate-mismatch-keeps-chunks
  discretion:
  - discretion-flac-encoder-binding
- id: contract-artifact-addressing
  verifications:
  - v-addressing-no-absolute-paths
  - v-addressing-relocation
  - v-addressing-identifier-only-segments
  - v-addressing-db-not-relocatable
  discretion: []
- id: contract-store-ownership
  verifications:
  - v-store-role-enforcement
  - v-store-busy-does-not-stall-capture
  - v-store-migration-forward-from-every-version
  - v-store-wal-config
  discretion:
  - discretion-migration-runner
- id: contract-retention-purge
  verifications:
  - v-purge-completeness
  - v-purge-idempotent
  - v-purge-cancels-inflight-steps
  - v-purge-interrupted-resumes
  discretion:
  - discretion-purge-walk-strategy
- id: contract-stable-identity
  verifications:
  - v-identity-recovery-reuse
  - v-identity-cross-surface-equality
  - v-identity-ordering
  discretion: []
- id: contract-workflow-step-idempotency
  verifications:
  - v-workflow-duplicate-enqueue-noop
  - v-workflow-lease-recovery-no-duplicate
  - v-workflow-config-change-new-step
  - v-workflow-edit-preservation
  discretion: []
- id: contract-processor-interface
  verifications:
  - v-processor-argv-no-shell
  - v-processor-staging-exact-contents
  - v-processor-capability-refusal
  - v-processor-model-digest
  discretion:
  - discretion-processor-host-framing
- id: contract-processor-budget
  verifications:
  - v-budget-progress-monotonic
  - v-budget-cancellation-bound
  - v-budget-cost-convergence
  - v-budget-overrun-is-warning
  discretion: []
- id: contract-destination-export-idempotency
  verifications:
  - v-export-crash-before-identity-record
  - v-export-duplicate-retry-no-duplicate
  - v-export-auth-failure-classification
  - v-export-offline-queue-survives-restart
  discretion: []
- id: contract-credential-custody
  verifications:
  - v-credential-no-secret-in-any-written-file
  - v-credential-type-not-displayable
  - v-credential-argv-free
  - v-credential-missing-is-typed
  discretion:
  - discretion-secret-zeroization
- id: contract-diagnostic-redaction
  verifications:
  - v-redaction-marker-scan
  - v-redaction-content-type-not-loggable
  - v-redaction-error-display-elides-payload
  discretion: []
- id: contract-egress-inventory
  verifications:
  - v-egress-inventory-complete
  - v-egress-inventory-no-first-party
  - v-egress-inventory-negative-fixture
  - v-egress-audit-matches-inventory
  discretion: []
- id: contract-release-manifest-trust
  verifications:
  - v-manifest-tampered-rejected
  - v-manifest-downgrade-rejected
  - v-manifest-unknown-key-rejected
  - v-manifest-digest-mismatch-no-activation
  discretion: []
- id: contract-docs-conformance
  verifications:
  - v-docs-schema-conformance
  - v-docs-adr-placement
  - v-docs-change-members-nonempty
  - v-docs-invariant-enforcement-named
  discretion: []
- id: contract-verification-tiering
  verifications:
  - v-tier-portable-suite-on-non-windows
  - v-tier-every-t2-registered
  - v-tier-ci-defines-both-gates
  - v-tier-windows-suite-green
  discretion: []
adrs:
- adr-20260903-capture-engine-process-isolation
- adr-20260903-workflow-runtime-process-topology
- adr-20260903-desktop-stack-and-ipc
- adr-20260903-workspace-boundary-enforcement
- adr-20260903-local-store-and-artifact-layout
- adr-20260903-audio-format-and-chunking
- adr-20260903-automatic-recording-modes
- adr-20260903-detector-signal-replay-contract
- adr-20260903-extension-localhost-channel-trust
- adr-20260903-workflow-identity-and-idempotency
- adr-20260903-initial-processor-adapters
- adr-20260903-local-transcription-budget
- adr-20260903-update-and-manifest-distribution
- adr-20260903-threat-model-and-credential-policy
- adr-20260903-phase0-executable-contract-skeleton
decision_dispositions:
- decision_input_ref: decision-input-no-proprietary-backend
  disposition: Adopted as a binding constraint and made mechanically checkable rather
    than asserted. Update metadata and adapter enablement are verified client-side
    by signature and OAuth uses installed-app PKCE, so no first-party service appears
    on any workflow path; and egress-inventory.toml enumerates every host reachable
    from source with a closed owner enum that has no first-party value to write, so
    adding one fails the build. The rejected baseline (a thin first-party relay for
    token exchange and update metadata) is recorded in the update ADR.
  adr_refs:
  - adr-20260903-update-and-manifest-distribution
  - adr-20260903-threat-model-and-credential-policy
  contract_refs:
  - contract-release-manifest-trust
  - contract-destination-export-idempotency
  - contract-egress-inventory
- decision_input_ref: decision-input-no-dom-detection
  disposition: Adopted as a binding constraint and made structural rather than aspirational
    - the signal envelope is a closed schema with no field capable of carrying UI-derived
    text, so a DOM-derived fact has nowhere to live. The rejected baseline (UI Automation
    and accessibility-tree probing) is recorded in the detector ADR.
  adr_refs:
  - adr-20260903-detector-signal-replay-contract
  contract_refs:
  - contract-signal-envelope
  - contract-detector-determinism
- decision_input_ref: decision-input-capture-engine-separate-process
  disposition: 'Adopted and closed further than the decision states. The engine is
    a per-user single-instance process that owns session truth, hosts the workflow
    runtime, and outlives every client. The consent-surface tension is resolved in
    the engine''s favour rather than against it: the engine raises its own OS notification,
    so automatic recording works with no client attached, and only the absence of
    every surface suppresses a start. The rejected form (requiring an attached client)
    is recorded because it would have disabled automatic recording in the exact case
    this decision exists to enable.'
  adr_refs:
  - adr-20260903-capture-engine-process-isolation
  - adr-20260903-workflow-runtime-process-topology
  - adr-20260903-automatic-recording-modes
  contract_refs:
  - contract-process-topology
  - contract-consent-surface-precondition
  - contract-processing-isolation
- decision_input_ref: decision-input-desktop-stack
  disposition: 'Adopted as decided (Rust engine with windows-rs, Tauri 2 UI, whisper.cpp
    and sherpa-onnx bindings). Alternatives (.NET 8 with WinUI 3, Electron with a
    Rust sidecar) recorded and rejected with reasons in the stack ADR. Two consequences
    the decision leaves implicit are fixed here: the Windows Credential Manager it
    names is the sole secret store, discharged by contract-credential-custody (this
    is where the competing draft''s separate os-credential-store input is subsumed);
    and the package identity it names is what lets the engine raise its own notification,
    which the consent-surface rule now depends on.'
  adr_refs:
  - adr-20260903-desktop-stack-and-ipc
  - adr-20260903-workflow-runtime-process-topology
  contract_refs:
  - contract-process-topology
  - contract-ipc-protocol
  - contract-credential-custody
  - contract-consent-surface-precondition
- decision_input_ref: decision-input-ipc-mechanism
  disposition: Adopted as decided (JSON-RPC over a named pipe). Because a named pipe
    carries no origin concept and JSON-RPC carries no resync, the plan adds a SID-checked
    ACL contract and a snapshot-plus-sequence resync rule. Alternatives (loopback
    TCP, shared-memory ring) recorded and rejected.
  adr_refs:
  - adr-20260903-desktop-stack-and-ipc
  contract_refs:
  - contract-ipc-protocol
  - contract-ipc-transport-authz
- decision_input_ref: decision-input-boundary-toolchain
  disposition: 'Adopted as decided, and strengthened with three additions required
    to make the exit criterion falsifiable: a two-class literal scan with a declared
    surface alongside the dependency-direction check, a negative fixture carrying
    decoys as well as violations so precision is proven and not only detection power,
    and the capture-path and native-linkage isolation rules that turn PLAN section
    7''s reliability guarantee into a graph property.'
  adr_refs:
  - adr-20260903-workspace-boundary-enforcement
  contract_refs:
  - contract-module-boundary-enforcement
  - contract-detector-determinism
  - contract-processing-isolation
- decision_input_ref: decision-input-db-artifact-layout
  disposition: 'Adopted as decided, with three derived constraints made explicit.
    Because the artifact root is user-configurable and may be a network share or removable
    drive, the database stays pinned under the local application-data directory and
    artifacts are addressed as a root identifier plus relative path. Because the workflow
    runtime now lives in the engine, the writer set is exactly two processes and the
    alternative of routing every write through the engine is recorded and rejected.
    And because the database is a projection of a directory that is the truth, deletion
    needs an owner: contract-retention-purge fixes the two-phase delete, the idempotent
    purge and the tombstone, while leaving the default grace value to Phase 2 per
    PLAN section 8.'
  adr_refs:
  - adr-20260903-local-store-and-artifact-layout
  - adr-20260903-workflow-runtime-process-topology
  contract_refs:
  - contract-artifact-addressing
  - contract-store-ownership
  - contract-retention-purge
- decision_input_ref: decision-input-audio-format
  disposition: Adopted as decided (16 kHz mono PCM WAV in 30 second chunks, consolidated
    to FLAC, optional Opus for sharing). The plan adds the durability ordering, the
    bounded loss window, and verify-before-delete consolidation. Alternatives (direct
    Opus capture, 48 kHz stereo archival) recorded and rejected.
  adr_refs:
  - adr-20260903-audio-format-and-chunking
  contract_refs:
  - contract-chunk-durability
  - contract-track-consolidation
  - contract-session-timeline
- decision_input_ref: decision-input-recording-modes
  disposition: 'Adopted as decided (auto, ask, manual with per-application override,
    10 second countdown, 60 second hysteresis). The plan supplies every timing semantic
    the decision leaves open, all as fixed numbers rather than placeholders: a suspend-excluding
    clock, re-evaluation after resume, a 60 second cancel quiet period per meeting
    identity, a 30 second still-in-the-meeting prompt granting one 300 second extension,
    and the consent-surface rule in its engine-first form. It also fixes the durable
    footprint: no audio byte reaches the artifact root before the recording state,
    so cancelling is observable on disk and not only in the interface.'
  adr_refs:
  - adr-20260903-automatic-recording-modes
  contract_refs:
  - contract-recording-mode-policy
  - contract-consent-surface-precondition
  - contract-session-state-machine
- decision_input_ref: decision-input-initial-adapters
  disposition: Adopted as decided. Phase 0 fixes only the contract these adapters
    must satisfy - capability declaration, staged inputs, argument-vector invocation,
    provenance, digest-pinned model files, and execution inside a per-job ma-processor-host
    child bounded by a job object, which is what makes it safe to host the workflow
    runtime in the engine. The rejected baselines (faster-whisper via a Python sidecar,
    cloud-only transcription) are recorded.
  adr_refs:
  - adr-20260903-initial-processor-adapters
  - adr-20260903-workflow-runtime-process-topology
  contract_refs:
  - contract-processor-interface
  - contract-processing-isolation
- decision_input_ref: decision-input-transcription-budget
  disposition: Adopted as decided (at most 1.0x real time on CPU, mandatory progress
    and cancellation, overrun is a warning). The plan adds a cost-convergence obligation
    so a naive accumulating-context implementation cannot pass, and fixes cancellation
    at five seconds, which is enforceable by construction because the work runs in
    a killable child process. The rejected baseline (overrun falls back to the external
    API) is recorded as violating explicit-transmission consent.
  adr_refs:
  - adr-20260903-local-transcription-budget
  contract_refs:
  - contract-processor-budget
  - contract-processing-isolation
- decision_input_ref: decision-input-update-manifest-distribution
  disposition: Adopted as decided (GitHub Releases, code-signed installer, Ed25519-signed
    manifests, Tauri updater). The plan adds rollback protection, key rotation, the
    verify-before-use rule and the deferral of an engine replacement while a session
    is non-terminal, none of which the decision states. The deferral rule is stated
    by contract-release-manifest-trust and its check is registered under contract-process-topology,
    which owns engine process lifetime.
  adr_refs:
  - adr-20260903-update-and-manifest-distribution
  contract_refs:
  - contract-release-manifest-trust
  - contract-egress-inventory
  - contract-process-topology
- decision_input_ref: decision-input-meet-extension-detection-only
  disposition: 'Adopted as decided, and the transport is now closed rather than preserved.
    Consequences fixed: extension signals are non-authoritative and require corroboration
    by an operating-system microphone signal from the same browser process tree; browser
    loopback tracks carry a contamination-risk flag; the channel is a loopback listener
    with a 5 second freshness window and a rate cap. The native messaging alternative
    is recorded as rejected with the reasons and with the named evidence that would
    reverse it.'
  adr_refs:
  - adr-20260903-extension-localhost-channel-trust
  - adr-20260903-detector-signal-replay-contract
  contract_refs:
  - contract-extension-channel-trust
  - contract-detector-outcome-partition
  - contract-session-timeline
- decision_input_ref: decision-input-drive-oauth-pkce
  disposition: 'Adopted as decided. Because the drive.file scope cannot discover objects
    the application did not create, export idempotency is built on the recorded remote
    identity plus an external-identifier marker rather than a name search, and the
    identity is committed as an intended effect-ledger row before the remote call
    so the post-crash lookup has a key to search for. Because those objects are the
    user''s own files, deletion never removes them: contract-retention-purge lists
    them from the tombstone instead.'
  adr_refs:
  - adr-20260903-workflow-identity-and-idempotency
  contract_refs:
  - contract-destination-export-idempotency
  - contract-credential-custody
  - contract-retention-purge
- decision_input_ref: decision-input-notion-internal-token
  disposition: Adopted as decided. The credential contract accommodates a long-lived
    non-refreshable secret whose only invalidation signal is a 401, classified as
    needs-reauthentication rather than retried.
  adr_refs:
  - adr-20260903-workflow-identity-and-idempotency
  - adr-20260903-threat-model-and-credential-policy
  contract_refs:
  - contract-credential-custody
  - contract-destination-export-idempotency
- decision_input_ref: decision-input-cli-adapter-postmvp
  disposition: Deferred as decided, but its admissibility preconditions are established
    now - no shell invocation and explicit file staging - because they are cheaper
    to establish than to retrofit and are independently required by the cross-cutting
    security rules.
  adr_refs:
  - adr-20260903-initial-processor-adapters
  contract_refs:
  - contract-processor-interface
- decision_input_ref: decision-input-transcription-languages
  disposition: Adopted as decided. The processor capability declaration carries an
    explicit language set and an unsupported-language request is a typed refusal rather
    than a silent best-effort transcription.
  adr_refs:
  - adr-20260903-initial-processor-adapters
  contract_refs:
  - contract-processor-interface
- decision_input_ref: decision-input-adr-schema-shape
  disposition: Adopted as an authoring constraint on all fifteen proposed ADRs - named
    decision makers and a tripolar consequences object with a genuinely non-empty
    negative list.
  adr_refs:
  - adr-20260903-phase0-executable-contract-skeleton
  contract_refs:
  - contract-docs-conformance
- decision_input_ref: decision-input-docs-lint-target-placement
  disposition: Adopted. ADRs are filed flat at docs/adr/adr-YYYYMMDD-slug.md because
    the lint glob is non-recursive; design documents may nest.
  adr_refs:
  - adr-20260903-phase0-executable-contract-skeleton
  contract_refs:
  - contract-docs-conformance
- decision_input_ref: decision-input-design-doc-schema
  disposition: Adopted. Only the discipline that binds future decisions is promoted
    to persistent design documents - module boundaries including the isolation rules,
    session lifecycle including the pre-recording footprint invariant, the recording
    and artifact model including deletion, and trust boundaries including the egress
    inventory - and each invariant must name a mechanical check where one exists.
  adr_refs:
  - adr-20260903-phase0-executable-contract-skeleton
  contract_refs:
  - contract-docs-conformance
- decision_input_ref: decision-input-change-package-members
  disposition: Adopted. The integrator materializes requirements, implementation and
    verification members; leaving a required member empty leaves the manifest unfulfilled
    and blocks closure.
  adr_refs:
  - adr-20260903-phase0-executable-contract-skeleton
  contract_refs:
  - contract-docs-conformance
- decision_input_ref: decision-input-status-transition-whitelist
  disposition: 'Adopted. Every ADR is created at proposed and this plan performs no
    acceptance transition; the change advances only through its whitelisted next state.
    This is a lifecycle constraint, not an open design question: the decisions themselves
    are made here, and the change package records the proposed-to-accepted transition
    as a gate that precedes the units implementing the contracts each ADR binds.'
  adr_refs:
  - adr-20260903-phase0-executable-contract-skeleton
  contract_refs:
  - contract-docs-conformance
milestones:
- id: foundation
- id: runtime
- id: processing
- id: security
- id: records
reference_algorithms: []
---

<!-- lifecycle is owned by change.md -->

# Implementation

The canonical plan (prose plus machine skeleton) is copied into `design-plan/` in this package. This member carries what an implementer needs to start: the contract inventory with its owners and requirements, the seams that make each contract testable, the unit order, and the boundaries that must not be crossed.

## Approach

Phase 0 delivers a contract-carrying skeleton rather than documents. Every cross-boundary shape exists twice
— as Rust types and as a JSON Schema under `contracts/` — with a conformance test round-tripping golden
fixtures so the two cannot drift. Behavioural seams get deterministic Phase 0 implementations that Phase 1 to
4 later replace with real ones, so Phase 0's tests survive: `CaptureSource` has a synthetic PCM source,
`SignalSource` has a fixture-replay source, and `Processor` and `Destination` have recording fakes. Everything
nominal is a policy file with a conformance test — `boundary.toml`, `verification-tiers.toml`,
`egress-inventory.toml`, the session transition table — so adding a violation fails continuous integration
rather than review.

## Process and crate topology

Three long-lived process kinds and one transient one, and this is the closed set:

| Process | Lifetime | Owns |
| --- | --- | --- |
| `ma-engine.exe` | logon to logoff, one per user | detection, capture, session truth, the workflow runtime, the export queue, and the `session` / `workflow` / `export` / `tombstone` store families |
| `app/ui` (Tauri 2) | user-launched, closable | consent and library surfaces and the `settings` store family; owns no session truth |
| `ma-processor-host.exe` | one per processing job | a single processor invocation and its staged inputs |
| browser extension host | per browser, detection only | forwarding non-authoritative tab signals |

Crate layers, bottom to top, where a crate may depend only on strictly lower layers and L4 is a sink that
only an L5 composition root may depend on:

| Layer | Crates |
| --- | --- |
| L0 kernel | `ma-core-types` |
| L1 contracts | `ma-signal`, `ma-ipc`, `ma-processor`, `ma-destination`, `ma-manifest`, `ma-secure` |
| L2 domain | `ma-session`, `ma-detect`, `ma-workflow` |
| L3 infrastructure | `ma-store`, `ma-capture`, `ma-signals-windows`, `ma-ext-channel` |
| L4 adapters | `ma-adapter-teams`, `ma-adapter-slack`, `ma-adapter-zoom`, `ma-adapter-meet`, `ma-processor-*`, `ma-destination-*` |
| L5 composition roots | `ma-engine`, `ma-processor-host`, `app/ui/src-tauri`, `xtask` |

Two additional forbidden-edge classes carry PLAN section 7's reliability guarantee: the capture-path crates
(`ma-core-types`, `ma-session`, `ma-capture`) may not reach `ma-workflow`, `ma-processor`, `ma-destination`
or any adapter crate; and only `ma-processor-host` and the processor adapters it loads may depend on a crate
that links a native inference library.

## Implementation contracts

Twenty-nine contracts. Each names its owning component, the requirements it discharges, the ADRs that justify it, and the units that carry it. Verification identifiers are listed in `verification.md`.

There is no separate compatibility contract. Each versioned surface carries its own compatibility rule and
its own check: the store schema in `contract-store-ownership`, the control protocol in
`contract-ipc-protocol`, the timeline fixture header in `contract-signal-envelope`, and the update and
adapter manifests in `contract-release-manifest-trust`.

| contract | dimension | owner | requirements | ADRs |
| --- | --- | --- | --- | --- |
| `contract-process-topology` | state_lifecycle | `component-capture-engine` | FR-004, FR-005, FR-017, NFR-003, FR-030, FR-025 | adr-20260903-capture-engine-process-isolation, adr-20260903-phase0-executable-contract-skeleton, adr-20260903-workflow-runtime-process-topology, adr-20260903-update-and-manifest-distribution |
| `contract-processing-isolation` | failure_recovery | `component-capture-engine` | NFR-009, FR-030, FR-004 | adr-20260903-workflow-runtime-process-topology, adr-20260903-workspace-boundary-enforcement |
| `contract-ipc-protocol` | integration_contract | `component-ipc-contract` | FR-005, FR-011, NFR-008 | adr-20260903-desktop-stack-and-ipc |
| `contract-ipc-transport-authz` | security_boundary | `component-security-policy` | FR-006, NFR-001 | adr-20260903-desktop-stack-and-ipc, adr-20260903-threat-model-and-credential-policy |
| `contract-session-state-machine` | state_lifecycle | `component-session-model` | FR-010, FR-011, FR-012, FR-014, FR-017, FR-027 | adr-20260903-automatic-recording-modes, adr-20260903-capture-engine-process-isolation |
| `contract-recording-mode-policy` | control_flow | `component-session-model` | FR-010, FR-012, NFR-005, FR-028 | adr-20260903-automatic-recording-modes |
| `contract-consent-surface-precondition` | user_observability | `component-session-model` | FR-011, FR-004, FR-027, FR-028 | adr-20260903-automatic-recording-modes, adr-20260903-capture-engine-process-isolation |
| `contract-signal-envelope` | data_model | `component-signal-contract` | FR-007, NFR-005 | adr-20260903-detector-signal-replay-contract |
| `contract-detector-determinism` | control_flow | `component-detector-core` | FR-007, NFR-005, NFR-008 | adr-20260903-detector-signal-replay-contract, adr-20260903-workspace-boundary-enforcement |
| `contract-detector-outcome-partition` | control_flow | `component-detector-core` | FR-008, FR-009, NFR-005 | adr-20260903-detector-signal-replay-contract, adr-20260903-extension-localhost-channel-trust |
| `contract-module-boundary-enforcement` | integration_contract | `component-boundary-check` | FR-002, FR-003, NFR-008, FR-001 | adr-20260903-workspace-boundary-enforcement |
| `contract-extension-channel-trust` | security_boundary | `component-extension-channel` | FR-009, NFR-001, NFR-005 | adr-20260903-extension-localhost-channel-trust, adr-20260903-threat-model-and-credential-policy |
| `contract-chunk-durability` | failure_recovery | `component-capture-engine` | FR-013, FR-014, NFR-003 | adr-20260903-audio-format-and-chunking |
| `contract-session-timeline` | data_model | `component-core-types` | FR-015, FR-017, FR-013 | adr-20260903-audio-format-and-chunking |
| `contract-track-consolidation` | data_model | `component-capture-engine` | FR-016, FR-013 | adr-20260903-audio-format-and-chunking |
| `contract-artifact-addressing` | data_model | `component-store` | FR-024, NFR-007, FR-017 | adr-20260903-local-store-and-artifact-layout |
| `contract-store-ownership` | concurrency | `component-store` | FR-017, FR-018, NFR-007 | adr-20260903-local-store-and-artifact-layout, adr-20260903-workflow-runtime-process-topology |
| `contract-retention-purge` | data_model | `component-store` | FR-029, FR-024, NFR-002 | adr-20260903-local-store-and-artifact-layout |
| `contract-stable-identity` | data_model | `component-core-types` | FR-017, FR-018, FR-023 | adr-20260903-workflow-identity-and-idempotency |
| `contract-workflow-step-idempotency` | state_lifecycle | `component-workflow-core` | FR-018, FR-019, FR-021 | adr-20260903-workflow-identity-and-idempotency |
| `contract-processor-interface` | integration_contract | `component-processor-contract` | FR-020, FR-021, NFR-001, NFR-006, FR-030 | adr-20260903-initial-processor-adapters, adr-20260903-threat-model-and-credential-policy, adr-20260903-workflow-runtime-process-topology |
| `contract-processor-budget` | performance_budget | `component-processor-contract` | FR-021, FR-022, NFR-004 | adr-20260903-local-transcription-budget |
| `contract-destination-export-idempotency` | integration_contract | `component-destination-contract` | FR-023, NFR-001, NFR-006 | adr-20260903-workflow-identity-and-idempotency |
| `contract-credential-custody` | security_boundary | `component-security-policy` | NFR-001, FR-020, FR-023 | adr-20260903-threat-model-and-credential-policy |
| `contract-diagnostic-redaction` | user_observability | `component-security-policy` | NFR-002, NFR-006, NFR-005 | adr-20260903-threat-model-and-credential-policy |
| `contract-egress-inventory` | security_boundary | `component-security-policy` | NFR-006, NFR-003 | adr-20260903-threat-model-and-credential-policy, adr-20260903-update-and-manifest-distribution |
| `contract-release-manifest-trust` | security_boundary | `component-release-supply-chain` | FR-025, NFR-003 | adr-20260903-update-and-manifest-distribution |
| `contract-docs-conformance` | integration_contract | `component-docs-artifacts` | FR-026 | adr-20260903-phase0-executable-contract-skeleton |
| `contract-verification-tiering` | integration_contract | `component-boundary-check` | NFR-010, NFR-008 | adr-20260903-phase0-executable-contract-skeleton |

## Seams that make the contracts testable

| Seam | Replaces | What it makes testable without the real thing |
| --- | --- | --- |
| `CaptureSource` + `SyntheticSource` | WASAPI | chunk boundaries, gaps, kill-recovery, consolidation, backpressure — with no audio hardware and on any host for the T1 parts |
| `SignalSource` + fixture replay | Windows collectors | detector determinism, the outcome partition, resync handling and clock-jump ordering, from committed JSONL fixtures |
| `Processor` + `ScriptedProcessor` | whisper.cpp, OpenAI, sherpa-onnx, Claude | slowness, uncancellable work, growing per-item cost, budget overrun, and an aborting host child |
| `Destination` + fake destination | Google Drive, Notion | the crash window between remote create and identity record, authentication failure classification, and backlog behaviour |
| injected process launcher | real child-process spawn | processor-host crash handling without a native library |
| in-memory duplex transport | the named pipe | the whole control-channel protocol, handshake and resync, leaving only access-control and squat tests on a real pipe |
| boundary checker over a fixture workspace | the real crate graph | that the checker detects planted violations and does not report planted decoys |
| purge job over a temp artifact root | the real artifact root | deletion completeness, idempotence and mid-walk resumption |

## Store families and writers

| Family | Tables | Writer |
| --- | --- | --- |
| session | `session`, `session_transition`, `track`, `chunk`, `gap` | `ma-engine.exe` |
| workflow | `workflow_step`, `work_item`, `effect_ledger`, `artifact`, `generation`, `edit_overlay` | `ma-engine.exe` |
| export | `export`, `export_attempt`, `egress_audit` | `ma-engine.exe` |
| tombstone | `tombstone` | `ma-engine.exe` (purge job) |
| settings | `settings`, `app_mode_override`, `roots` | interface host |

Reads are unrestricted for both processes; only writes carry a role. Interface mutations outside `settings`
go through the control-channel methods `artifact.edit` and `meeting.delete`.

## Unit order

| # | unit | depends on | contracts |
| --- | --- | --- | --- |
| 1 | `workspace-and-boundary-scaffold` | — | `contract-module-boundary-enforcement`, `contract-detector-determinism`, `contract-processing-isolation`, `contract-verification-tiering` |
| 2 | `core-types-and-identity` | — | `contract-stable-identity`, `contract-session-timeline`, `contract-artifact-addressing` |
| 3 | `persistence-and-artifact-layout` | `core-types-and-identity` | `contract-artifact-addressing`, `contract-store-ownership`, `contract-stable-identity`, `contract-retention-purge` |
| 4 | `session-state-machine` | `core-types-and-identity` | `contract-session-state-machine`, `contract-recording-mode-policy`, `contract-consent-surface-precondition` |
| 5 | `signal-and-detector-contracts` | `core-types-and-identity` | `contract-signal-envelope`, `contract-detector-determinism`, `contract-detector-outcome-partition` |
| 6 | `service-adapter-skeletons` | `signal-and-detector-contracts` | `contract-module-boundary-enforcement`, `contract-detector-outcome-partition` |
| 7 | `extension-channel-contract` | `signal-and-detector-contracts` | `contract-extension-channel-trust` |
| 8 | `ipc-contract-and-engine-process` | `persistence-and-artifact-layout`, `session-state-machine` | `contract-ipc-protocol`, `contract-ipc-transport-authz`, `contract-process-topology` |
| 9 | `capture-recording-durability` | `persistence-and-artifact-layout`, `ipc-contract-and-engine-process` | `contract-chunk-durability`, `contract-session-timeline`, `contract-track-consolidation`, `contract-process-topology`, `contract-processing-isolation` |
| 10 | `workflow-core-contract` | `persistence-and-artifact-layout` | `contract-workflow-step-idempotency`, `contract-stable-identity`, `contract-retention-purge` |
| 11 | `processor-contract` | `workflow-core-contract` | `contract-processor-interface`, `contract-processor-budget`, `contract-processing-isolation` |
| 12 | `destination-contract` | `workflow-core-contract` | `contract-destination-export-idempotency`, `contract-diagnostic-redaction`, `contract-egress-inventory` |
| 13 | `security-and-credential-policy` | `core-types-and-identity` | `contract-credential-custody`, `contract-diagnostic-redaction`, `contract-ipc-transport-authz`, `contract-egress-inventory` |
| 14 | `release-supply-chain` | `security-and-credential-policy` | `contract-release-manifest-trust` |
| 15 | `ui-shell-consent-surface` | `ipc-contract-and-engine-process` | `contract-consent-surface-precondition`, `contract-ipc-protocol` |
| 16 | `docs-and-adr-materialization` | `workspace-and-boundary-scaffold`, `session-state-machine`, `signal-and-detector-contracts`, `capture-recording-durability`, `processor-contract`, `destination-contract`, `security-and-credential-policy`, `release-supply-chain` | `contract-docs-conformance` |

## Unit objectives and boundaries

### `workspace-and-boundary-scaffold`

Create the cargo workspace with declared layers, make the module boundary and processing-isolation rules mechanically enforced and provably non-vacuous, and make verification tiering a build-time obligation rather than a convention.

**Files.** `Cargo.toml`, `rust-toolchain.toml`, `boundary.toml`, `verification-tiers.toml`, `deny.toml`, `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/boundary.rs`, `xtask/src/verify.rs`, `xtask/tests/boundary_negative.rs`, `xtask/tests/isolation_negative.rs`, `xtask/tests/fixtures/violating-workspace/**`, `.github/workflows/ci.yml`

**Output.** Rust workspace manifests, a declarative boundary.toml and verification-tiers.toml, an xtask binary with boundary and verify subcommands, a fixture workspace carrying violations and decoys, and a CI workflow with two required jobs.

**Guidance.** Resolve the graph from cargo metadata with all features enabled; check transitive edges, not only direct ones; tokenize sources so identifiers, string literals and comments are distinguishable rather than grepping raw bytes; report all violations in one pass rather than stopping at the first.

**Boundaries.** Do not implement any product behaviour; do not add crates beyond empty layer placeholders needed for the graph; do not weaken the negative fixture to make the checker pass.

**Done when.**

- cargo xtask boundary exits 0 on the clean workspace and reports the number of edges checked
- the negative fixture produces a non-zero exit naming exactly the planted forbidden edge, forbidden literal and forbidden import, and reports none of the three planted decoys (a doc comment containing 'must meet', a graph_edge binding, a "meeting ended" literal)
- a feature-gated dependency from a non-composition-root crate onto an adapter crate is detected, and so are a capture-path edge onto ma-workflow and a native-inference dependency on ma-engine
- cargo xtask verify --check-registration fails when a T2 verification id is absent from verification-tiers.toml, and CI defines both the portable and the windows required jobs

### `core-types-and-identity`

Fix the shared vocabulary of identifiers, session timeline arithmetic, artifact references and errors that every other crate depends on.

**Files.** `crates/ma-core-types/Cargo.toml`, `crates/ma-core-types/src/id.rs`, `crates/ma-core-types/src/timeline.rs`, `crates/ma-core-types/src/artifact_ref.rs`, `crates/ma-core-types/src/error.rs`

**Output.** A dependency-free Rust library crate with property tests.

**Guidance.** Use proptest for the tiling and ordering invariants; keep the crate free of platform and I/O dependencies.

**Boundaries.** Do not add persistence, capture or IPC concerns here; do not introduce types that only one component uses.

**Done when.**

- identifier types are UUIDv7, time-ordered, and serialize identically in database, path and payload contexts
- chunks and gaps provably tile each track range with no overlap under a property test
- a timestamp computed after a missing chunk retains its true session position
- track descriptors carry capture_mode and contamination_risk

### `persistence-and-artifact-layout`

Define the relational state, its migration discipline, writer ownership and the relocatable artifact addressing model.

**Files.** `crates/ma-store/Cargo.toml`, `crates/ma-store/src/schema.rs`, `crates/ma-store/src/migration.rs`, `crates/ma-store/migrations/*.sql`, `crates/ma-store/src/repo/**`, `crates/ma-store/src/purge.rs`

**Output.** A Rust crate with embedded SQL migrations and repository modules, plus tests against temporary directories.

**Guidance.** Configure WAL, busy_timeout and foreign keys explicitly and assert the configuration in a test; use BEGIN IMMEDIATE for read-modify-write.

**Boundaries.** Do not implement workflow or export logic here; do not store any absolute path; do not make the database path configurable.

**Done when.**

- no inserted row contains an absolute artifact path or a drive or UNC prefix
- relocating the artifact root updates one row and leaves every reference resolvable
- a write to a table outside the connection role's family is rejected
- migrations apply forward from every released version and a newer database is refused with a typed error
- deleting a meeting hides it in one transaction and, after the purge job runs, the meeting_id appears nowhere under the artifact root and in no row outside tombstone
- a purge killed mid-walk resumes on restart and a second run is a no-op returning success

### `session-state-machine`

Fix the meeting-session lifecycle, the automatic recording mode policy, deadline semantics and the consent precondition as a pure, exhaustively testable function.

**Files.** `crates/ma-session/Cargo.toml`, `crates/ma-session/src/state.rs`, `crates/ma-session/src/transition_table.rs`, `crates/ma-session/src/mode.rs`, `crates/ma-session/src/deadline.rs`, `contracts/session/transitions.json`

**Output.** A pure Rust crate plus a JSON transition table used as the conformance source of truth.

**Guidance.** Pass time in as an argument rather than reading a clock; keep every effect a returned value rather than a side effect.

**Boundaries.** Do not perform I/O, start capture, or talk to the store or IPC from this crate; do not encode any service-specific knowledge.

**Done when.**

- the exported transition table equals contracts/session/transitions.json
- step is total over the state and event space, returning Rejected where no transition is declared
- a suspend and resume spanning a countdown re-evaluates instead of firing
- an automatic start decision with no consent surface of either kind - no deliverable engine notification and no attached client - produces a suppression record and no capture effect, while a deliverable notification alone is sufficient to arm
- no audio sample is written under the artifact root while the session is in candidate or arming, and a cancelled countdown leaves the meeting directory with zero chunk files
- a detection in ask mode returns a notify effect whose action set carries start, so ask mode is satisfiable with no client attached, and returns a suppression with cause no_consent_surface when neither surface can present it

### `signal-and-detector-contracts`

Fix what a signal is, how timelines are recorded and replayed, and make the detector a pure evidence-citing function with a closed outcome partition.

**Files.** `crates/ma-signal/Cargo.toml`, `crates/ma-signal/src/envelope.rs`, `crates/ma-signal/src/source.rs`, `crates/ma-signal/src/timeline.rs`, `contracts/signal/signal-envelope.schema.json`, `crates/ma-detect/Cargo.toml`, `crates/ma-detect/src/detector.rs`, `crates/ma-detect/src/adapter.rs`, `crates/ma-detect/src/decision.rs`, `crates/ma-detect/src/outcome.rs`, `fixtures/signal-timelines/**`

**Output.** Two Rust crates, a JSON Schema, and a set of committed replayable timeline fixtures.

**Guidance.** Enforce detector purity through the boundary check's forbidden-import list rather than review; use ordered collections or explicit sorts everywhere a decision order is observable.

**Boundaries.** Do not implement Windows collectors or detection heuristics here; do not put any service name in either crate.

**Done when.**

- the signal schema contains no free-text subject field capable of carrying UI-derived text
- replaying a fixture yields byte-identical decisions across repeated runs and a fresh process
- every decision cites at least one signal identifier and a rule identifier
- the outcome partition is exhaustive and an extension-authority signal alone never yields a determinate start

### `service-adapter-skeletons`

Establish the adapter seam and four service-specific data-only adapters that hold every service identifier.

**Files.** `crates/ma-adapter-teams/**`, `crates/ma-adapter-slack/**`, `crates/ma-adapter-zoom/**`, `crates/ma-adapter-meet/**`

**Output.** Four small Rust crates implementing one trait, with a shared conformance test suite.

**Guidance.** Keep each adapter a declarative table plus a match function; do not let adapters depend on each other.

**Boundaries.** Do not add detection heuristics or version-specific workarounds beyond placeholders; do not import platform APIs here.

**Done when.**

- each adapter crate is a graph sink depended on only by composition roots
- a shared adapter conformance suite passes for all four adapters
- a panicking adapter is disabled with a diagnostic and does not fail the detection pipeline
- all service identifiers appear only inside these crates and their fixtures

### `extension-channel-contract`

Fix the detection-only browser channel's message schema, authentication and non-authoritative status.

**Files.** `crates/ma-ext-channel/Cargo.toml`, `crates/ma-ext-channel/src/server.rs`, `crates/ma-ext-channel/src/auth.rs`, `crates/ma-ext-channel/src/message.rs`, `contracts/extension-channel/message.schema.json`

**Output.** A Rust crate with an injected transport, plus a JSON Schema for the message.

**Guidance.** Make the transport injectable so authentication and rejection paths are testable without a browser; record the native-messaging alternative in the ADR before committing to loopback.

**Boundaries.** Do not build the browser extension itself; do not accept any audio or DOM content over this channel.

**Done when.**

- a request without the token, with a web origin, or with a stale sequence is rejected and produces no signal
- the endpoint descriptor file is created with an owner-only ACL and the token is regenerated per engine start
- accepted messages become signals carrying host and tab key only, never a full URL or title
- a forged extension signal without a corroborating microphone signal does not start capture

### `ipc-contract-and-engine-process`

Fix the engine control channel and stand up the engine process as the single per-user authority for session state.

**Files.** `crates/ma-ipc/Cargo.toml`, `crates/ma-ipc/src/protocol.rs`, `crates/ma-ipc/src/method.rs`, `crates/ma-ipc/src/event.rs`, `crates/ma-ipc/src/transport.rs`, `crates/ma-ipc/src/dispatch.rs`, `crates/ma-ipc/src/authz.rs`, `contracts/ipc/protocol.schema.json`, `contracts/ipc/methods.schema.json`, `crates/ma-engine/Cargo.toml`, `crates/ma-engine/src/main.rs`, `crates/ma-engine/src/supervisor.rs`

**Output.** A transport-agnostic protocol crate, JSON Schemas with golden fixtures, and an engine binary with a single-instance lock.

**Guidance.** Drive both sides over an in-memory duplex for protocol tests and use a real named pipe only for the ACL and squat tests.

**Boundaries.** Do not implement capture or detection inside the engine binary beyond wiring the seams; do not add any method whose effect is not observable in a subsequent snapshot.

**Done when.**

- Rust types and JSON Schemas round-trip every golden fixture
- a major protocol mismatch refuses the connection with a typed error naming the required version
- a stalled client is either disconnected or detects the sequence gap and re-snapshots, and never renders stale state
- a connection from a different user SID is refused before method dispatch, and a pre-squatted pipe name causes engine exit rather than silent joining
- an update offered while a session is non-terminal leaves the running engine binary in place and applies only after the session terminates

### `capture-recording-durability`

Make durable recording, honest recovery and lossless consolidation real and testable behind a synthetic capture source.

**Files.** `crates/ma-capture/Cargo.toml`, `crates/ma-capture/src/source.rs`, `crates/ma-capture/src/chunk_writer.rs`, `crates/ma-capture/src/manifest.rs`, `crates/ma-capture/src/recovery.rs`, `crates/ma-capture/src/consolidate.rs`, `contracts/artifact/chunk-manifest.schema.json`

**Output.** A Rust crate with a CaptureSource seam, a deterministic synthetic source, and integration tests that kill processes.

**Guidance.** Order durability as flush, rename, manifest append, fsync; use a fault-injecting filesystem fake for backpressure and disk-full paths.

**Boundaries.** Do not implement WASAPI or any real device access; do not let the writer touch the database, IPC or the network.

**Done when.**

- killing the engine mid-chunk loses at most the in-progress chunk and recovery repairs or gaps the partial file
- the chunk directory is treated as truth and the manifest is reconciled in both directions
- a stalling filesystem produces an explicit gap and a degraded event rather than stalling the capture callback
- consolidated FLAC decodes sample-identically before any chunk is deleted, and a crash between verification and deletion re-runs idempotently
- aborting a scripted processor host child mid-job during a synthetic recording leaves chunk cadence unchanged and the session in recording

### `workflow-core-contract`

Fix step identity, idempotency, retry classification, artifact lifecycle and the separation of generated content from user edits.

**Files.** `crates/ma-workflow/Cargo.toml`, `crates/ma-workflow/src/step.rs`, `crates/ma-workflow/src/queue.rs`, `crates/ma-workflow/src/retry.rs`, `crates/ma-workflow/src/lifecycle.rs`, `crates/ma-workflow/src/edits.rs`, `crates/ma-workflow/src/effect_ledger.rs`

**Output.** A Rust crate driven in tests by recording fake processors and destinations.

**Guidance.** Commit the effect ledger's intended row before any effect outside the state database and update it to applied afterwards; compose generation plus overlay at read time rather than materialising the merged text; decompose transcription into per-chunk work items with stable identifiers.

**Boundaries.** Do not implement any processor or destination here; do not let a processing failure reach the capture path.

**Done when.**

- enqueueing a completed step key returns the recorded result and executes nothing
- a lease-expired running step is re-run without producing a duplicate artifact
- changing processor version or configuration produces a new step and retains the previous result
- an effect ledger row left at intended by a kill is resolved by lookup or by an explicit user decision on restart, never by a silent recreate
- regeneration adds a generation row and never mutates edit_overlay; an edit whose anchor is gone is retained with orphaned = true and is enumerable, and an edit offered with no anchor basis is refused

### `processor-contract`

Fix the replaceable processing seam including capability declaration, input isolation, invocation safety, provenance, progress and the time budget.

**Files.** `crates/ma-processor/Cargo.toml`, `crates/ma-processor/src/lib.rs`, `crates/ma-processor/src/capability.rs`, `crates/ma-processor/src/staging.rs`, `crates/ma-processor/src/progress.rs`, `crates/ma-processor/src/failure.rs`, `contracts/processor/processor-manifest.schema.json`, `crates/ma-processor/src/host.rs`, `crates/ma-processor-host/Cargo.toml`, `crates/ma-processor-host/src/main.rs`

**Output.** A Rust crate with a scripted fake processor able to simulate slowness, uncancellable work, growth in per-item cost and budget overrun.

**Guidance.** Assert the child process command line in tests rather than trusting the construction code; measure cancellation as an interval, not as a flag being set.

**Boundaries.** Do not implement whisper.cpp, OpenAI, sherpa-onnx or Claude adapters in this phase; do not accept a shell command as configuration.

**Done when.**

- a hostile configuration value is either type-rejected or passed as a single literal argument, and no shell is ever invoked
- the staging directory contains exactly the declared inputs and is removed after the job
- progress is monotonic, cancellation is observed within the declared bound, and per-item cost does not grow across a 240-item run
- a budget overrun emits a warning and the step still succeeds; a model digest mismatch is a permanent failure
- a processor that loads a native library or runs an external program executes inside ma-processor-host, and a scripted host that aborts yields HostCrashed rather than affecting the engine
- a scripted host that stays alive but emits no progress frame for 150 seconds is killed and the step is Retryable{no_progress} with its completed work items preserved, which is a different outcome from HostCrashed

### `destination-contract`

Fix the replaceable export seam, export identity, retry classification and the local egress audit.

**Files.** `crates/ma-destination/Cargo.toml`, `crates/ma-destination/src/lib.rs`, `crates/ma-destination/src/identity.rs`, `crates/ma-destination/src/retry.rs`, `crates/ma-destination/src/audit.rs`, `contracts/destination/destination-descriptor.schema.json`

**Output.** A Rust crate with a fake destination that can simulate the crash window and authentication failures.

**Guidance.** Persist the resumable session or external identifier before the create completes; treat the recorded identity as the only discovery mechanism under the drive.file scope.

**Boundaries.** Do not implement Google Drive or Notion clients in this phase; never delete or degrade a local artifact because an export failed.

**Done when.**

- a crash between remote creation and identity recording is reconciled by external-identifier lookup and creates no duplicate
- authentication failures are classified as needs-reauthentication rather than retried blindly
- the persistent export queue survives restart and has a declared backlog cap with a surfaced state
- every send appends an audit record containing identifiers and counts only
- a send to a host absent from egress-inventory.toml is rejected before the request and recorded, and the backlog cap of 500 surfaces the dropped export rather than silently refusing work

### `security-and-credential-policy`

Fix secret custody, log redaction, ACL construction and the documented threat model and trust boundaries.

**Files.** `crates/ma-secure/Cargo.toml`, `crates/ma-secure/src/secret.rs`, `crates/ma-secure/src/credential_store.rs`, `crates/ma-secure/src/redaction.rs`, `crates/ma-secure/src/acl.rs`, `docs/design/threat-model.md`, `docs/design/credential-policy.md`, `egress-inventory.toml`, `crates/ma-secure/tests/egress_inventory.rs`

**Output.** A Rust crate with compile-fail tests and a leak-scanning system test, plus two design documents.

**Guidance.** Make redaction a type-level property rather than a logging convention; scan the whole written-file set for planted markers rather than inspecting selected logs.

**Boundaries.** Do not add a secondary secret cache anywhere; do not pass secrets through process arguments.

**Done when.**

- a planted secret and planted meeting content appear in no file the application writes, including panic output and parse errors
- the secret type provably cannot be displayed or serialized in raw form, enforced by a compile-fail test
- constructed pipe and file security descriptors grant the owning user only
- a missing credential produces a typed needs-authentication result with the feature disabled and surfaced
- every host reachable from workspace source or from a contracts/ manifest appears in egress-inventory.toml, every entry declares a closed integration_owner, and both an undeclared host and a stale entry fail with distinct codes

### `release-supply-chain`

Fix the signed update and adapter manifest trust model, including rollback protection and key rotation, with no server-side trust decision.

**Files.** `crates/ma-manifest/Cargo.toml`, `crates/ma-manifest/src/manifest.rs`, `crates/ma-manifest/src/verify.rs`, `crates/ma-manifest/src/rollback.rs`, `crates/ma-manifest/src/keys.rs`, `contracts/manifest/update-manifest.schema.json`, `contracts/manifest/adapter-manifest.schema.json`, `.github/workflows/release.yml`

**Output.** A Rust crate taking bytes and a key set as arguments, JSON Schemas, and a release workflow.

**Guidance.** Verify first and parse second so an unverified document cannot influence control flow; test the captive-portal HTML response case explicitly.

**Boundaries.** Do not introduce any first-party service for update metadata or token exchange; do not auto-approve downgrades.

**Done when.**

- tampered, downgraded, unknown-key and digest-mismatched manifests are all rejected with distinct typed codes
- no manifest-declared value is used, including in logs, before verification succeeds
- a key rollover block signed by the current key introduces the next key and an unknown-key-only manifest is refused
- an engine replacement is deferred while any session is non-terminal

### `ui-shell-consent-surface`

Provide the consent and visibility surface as a thin client of the engine, owning no session truth.

**Files.** `app/ui/src-tauri/Cargo.toml`, `app/ui/src-tauri/src/main.rs`, `app/ui/src-tauri/src/engine_client.rs`, `app/ui/src-tauri/tauri.conf.json`, `app/ui/src/**`

**Output.** A Tauri 2 application skeleton with the engine client factored into a testable Rust module.

**Guidance.** Keep all reconnect and resync logic in the Rust client module so it is covered by headless tests.

**Boundaries.** Do not derive session state in the frontend; do not implement the meeting library, playback or settings beyond what the consent surface requires.

**Done when.**

- the UI renders only engine-supplied state and re-snapshots after any disconnect or sequence gap
- the countdown and its cancel affordance are driven by engine events, not by a local timer
- the client declares indicator and cancel capabilities at handshake, and automatic recording still starts when no client is running at all
- the engine client library is testable headlessly without WebView2

### `docs-and-adr-materialization`

Record Phase 0's decisions and durable discipline in the repository's documentation system without duplicating the same content across ADR, design and change package.

**Files.** `docs/adr/**`, `docs/design/module-boundaries.md`, `docs/design/session-lifecycle.md`, `docs/design/recording-artifact-model.md`, `docs/design/threat-model.md`, `docs/design/credential-policy.md`, `docs/changes/change-20260903-phase0-repository-and-contracts/change.md`, `docs/changes/change-20260903-phase0-repository-and-contracts/requirements.md`, `docs/changes/change-20260903-phase0-repository-and-contracts/implementation.md`, `docs/changes/change-20260903-phase0-repository-and-contracts/verification.md`

**Output.** Markdown documents with schema-conformant frontmatter.

**Guidance.** Let ADRs own the reasons, design documents own the current discipline, and the change package own the generation context; validate with the dev-docs conformance tooling before closing.

**Boundaries.** Do not set any ADR to accepted; do not move the change status past its whitelisted next state; do not copy implementation file inventories into persistent design documents.

**Done when.**

- every ADR sits at docs/adr/adr-YYYYMMDD-slug.md, declares non-empty decision_makers, uses the tripolar consequences object with all three lists non-empty, and starts at proposed
- exactly five design documents exist — module-boundaries, session-lifecycle, recording-artifact-model, threat-model and credential-policy — each validating against design.schema.json with every invariant naming a mechanical check where one exists
- the three required change members are materialized and non-empty
- the change root promotion manifest declares none with a reason, and no promotion entry names a target, because promotion upserts stable items of an existing design document and this change creates the repository's first ones

## Delegated implementation discretion

The following choices are private to a single unit, reversible, and pinned by a contract and a mechanical
check. They belong to the implementer, who escalates only on the listed conditions.

| discretion | unit | question | escalate when |
| --- | --- | --- | --- |
| `discretion-jsonrpc-dispatch` | `ipc-contract-and-engine-process` | How are JSON-RPC methods registered and dispatched inside the transport-agnostic protocol layer? | the wire framing, method set, error codes or event ordering guarantees would change; dispatch would introduce a blocking path that can stall event publication |
| `discretion-state-machine-representation` | `session-state-machine` | Is the state machine represented as an interpreted transition table or as typestate-style enums internally? | the exported transition table would no longer be derivable from the code; any declared transition would become unreachable from step; a transition could be applied without recording its cause |
| `discretion-ui-state-store` | `ui-shell-consent-surface` | Which frontend state-management approach holds the rendered session view inside the webview? | the UI would derive session state locally instead of rendering the engine snapshot; the indicator or the cancel affordance would depend on frontend state that can diverge from the engine; reconnect or resync logic would move out of the testable Rust engine client into the webview |
| `discretion-boundary-check-graph-source` | `workspace-and-boundary-scaffold` | How does the checker obtain and traverse the crate graph and tokenize sources into identifiers, string literals and comments? | any violation class present in the negative fixture would become undetectable; the graph would be resolved with anything other than all features enabled; transitive edges would stop being checked; the declared scan surface would change (class A identifier tokens, class B whole string literals, comments never); any decoy in the negative fixture would start being reported |
| `discretion-chunk-writer-buffering` | `capture-recording-durability` | How does the chunk writer buffer samples between the capture callback and the filesystem? | the bounded loss window would exceed one in-progress chunk; the declared backpressure queue depth or the gap-record semantics would change; the capture callback could block on the filesystem |
| `discretion-flac-encoder-binding` | `capture-recording-durability` | Which FLAC encoder implementation is used for track consolidation? | the choice adds a C toolchain requirement to the standard build; the dependency's licence class is not already permitted by deny.toml; the encoder cannot encode 16 kHz mono losslessly |
| `discretion-migration-runner` | `persistence-and-artifact-layout` | Which embedded migration runner applies the ordered SQL migrations? | migrations would stop being forward-only; user_version would stop being the schema version carrier; the migrate-from-every-released-version test could not be expressed |
| `discretion-purge-walk-strategy` | `persistence-and-artifact-layout` | How does the purge job walk and remove the meeting directory and its derived rows? | the purge would stop being resumable from deleted_at alone; a tombstone could be written while any byte of the meeting remains; a partially purged meeting could become visible again; the walk would proceed past an unresolved intended effect-ledger row |
| `discretion-processor-host-framing` | `processor-contract` | How are the frames encoded between the engine supervisor and the processor host child? Their contents and order — one verified request in, zero or more progress frames then exactly one result frame out — are fixed by `contract-processor-interface`; only the encoding is delegated, and only because both endpoints ship in one installer and are replaced together | a host crash would become indistinguishable from a normal exit; cancellation could exceed the five-second bound; progress could regress or stop being observable at least once per work item; any secret would move into the child's argument vector; the two endpoints would stop shipping as one installed unit |
| `discretion-secret-zeroization` | `security-and-credential-policy` | Which zeroization mechanism does the secret wrapper use on drop? | the type would gain a Display, Debug or Serialize implementation that reveals the value; exposing the inner value would stop being an explicit call at the transmission site; the mechanism would require the secret to be copied into an additional buffer that outlives the wrapper |

## Sequencing notes

Units 1 and 2 gate everything. Units 5 to 7 and 9 to 14 are largely parallel once 1 to 4 land. Unit 16 is
last because it records what the others fixed, but its schema constraints apply from the start: no ADR may be
created outside `docs/adr/` and none may be created at a status other than `proposed`.

Acceptance of the fifteen ADRs is a gate before unit 1 starts. The contracts below are written against
decisions those ADRs record, so implementing them before the decisions are accepted would build against a
proposal.
