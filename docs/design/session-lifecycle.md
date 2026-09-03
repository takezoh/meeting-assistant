---
id: design-session-lifecycle
kind: design
title: "Session lifecycle"
summary: The session state machine, the mode policy, the consent-surface precondition, the fixed deadlines, and how session truth is exposed over the control channel.
status: active
created: '2026-09-03'
scope_type: component
responsibilities:
  - id: RESP-001
    statement: "Own the single session state machine and its declared transition table."
  - id: RESP-002
    statement: "Resolve the effective mode (auto, ask, manual) per meeting identity and gate arming on a live consent surface."
  - id: RESP-003
    statement: "Expose session truth to clients as an authoritative snapshot plus sequenced transition events."
invariants:
  - id: INV-001
    statement: "Every accepted transition is declared in contracts/session/transitions.json and the Rust step function is total over it (v-session-table-conformance, v-session-exhaustive-step)."
    enforcement: test
  - id: INV-002
    statement: "No automatic capture begins without a consent surface at arming and at countdown expiry, and a suppressed decision is always recorded with its cause (v-consent-no-surface-no-start, v-consent-engine-notification-starts-without-client)."
    enforcement: test
  - id: INV-003
    statement: "A cancelled countdown leaves no audio byte on disk and suppresses re-arming for the same identity for the quiet period (v-consent-cancel-leaves-no-audio-byte, v-mode-countdown-cancel-suppression)."
    enforcement: test
  - id: INV-004
    statement: "Session truth is written only by the engine; a client renders only engine-supplied state and re-snapshots after a gap (v-ipc-resync-after-stall)."
    enforcement: test
  - id: INV-005
    statement: "Every deadline is a fixed number: 10 s countdown, 60 s cancel quiet period, 60 s end hysteresis, 30 s prompt, 300 s extension (v-mode-hysteresis-flap)."
    enforcement: test
boundaries:
  provides:
    - the SessionState, State and Event types and the step function in ma-session
    - the session.snapshot and session.transition wire shapes in ma-ipc
  consumes:
    - detector decisions from ma-detect
    - user commands and client capabilities from ma-ipc
    - notification delivery results from the platform
  forbidden:
    - session state derived in a client
    - a transition that is not in the declared table
    - a start in auto mode with no consent surface
variability:
  fixed:
    - the state set and the transition table
    - the deadline values
    - the consent-surface rule and its asymmetry (start unobserved forbidden, continue unobserved required)
  free:
    - the notification platform used for the engine's own consent surface
    - the per-application default modes
capabilities:
  - id: cap:session-authority
    uniqueness: global
failure_responsibilities:
  - id: FR-001
    statement: "An undeclared (state, event) pair is rejected and changes nothing; it is never a panic and never a silent transition."
  - id: FR-002
    statement: "Loss of every consent surface mid-countdown cancels the countdown with cause consent_surface_lost; loss during recording keeps recording and records indicator_unavailable."
  - id: FR-003
    statement: "An engine restart recovers the session from durable state and marks an interrupted recording interrupted rather than pretending it completed."
trust_boundaries:
  - id: TB-001
    statement: "engine to client: a client may send commands and receive snapshots and events; it cannot assert state."
compatibility_policies:
  - id: CP-001
    statement: "A new state or event is added to transitions.json first, and the table-matches-code test must be updated in the same change."
  - id: CP-002
    statement: "A changed deadline is an ADR-level decision because it changes user-visible behaviour promised in PLAN."
tags: [session, detection, consent]
owners: [take]
relations:
  - type: originatedFrom
    target: change-20260903-phase0-repository-and-contracts
source_paths:
  - crates/ma-session/src/state.rs
  - crates/ma-session/src/mode.rs
  - crates/ma-session/src/deadline.rs
  - contracts/session/transitions.json
---

## Purpose

One meeting is one session, and the session's state is the one fact the whole product agrees on. This
document states the states, what moves the session between them, when a detected meeting may start
recording automatically, and how that truth reaches a client.

## Responsibilities

`ma-session` owns the state machine (`step`), the mode policy (`ResolvedMode` from global, class and
per-adapter settings) and the deadlines. `ma-ipc` and `ma-engine` own the wire and the process that hold the
truth; `app/ui` renders it.

## Boundaries

States: `idle`, `candidate`, `arming`, `recording`, `paused`, `ending`, `finalizing`, `completed`,
`discarded`, `interrupted`, `failed`. A detector start raises a candidate; the mode policy decides whether
it arms (auto), prompts (ask) or waits for a manual start; a countdown of ten seconds with a cancel affordance
precedes automatic capture; an end decision opens a sixty-second hysteresis window in which a continuing
signal returns to recording without a new session. A paused session never resumes capture on a detector signal: an end signal starts the hysteresis window while it stays paused, expiry finalizes what was recorded, and only the user's resume command brings capture back.

## Invariants

The transition table in `contracts/session/transitions.json` is the contract; the Rust step function is
generated against it and tested to match. The consent-surface precondition is asymmetric on purpose: starting
unobserved is forbidden, continuing unobserved is required.

## Collaboration

Detector decisions arrive as events with their decision ids as cause references; commands arrive from clients
over `ma-ipc` with the client name as cause; timers arrive from the engine's deadline scheduler. Every
transition is recorded with its cause and is published as a `session.transition` event with a per-connection
sequence number.

## Failure Responsibility

A rejected event is an observable `Rejected` outcome on the wire, not a no-op. Surface loss and engine
restart have the named outcomes above.

## Variability

Fixed: the table, the deadlines, the consent rule. Free: the notification platform and the per-application
default modes.

## Conformance

`cargo test -p ma-session` (table match, undeclared pairs, cancel suppression, deadlines),
`cargo test -p ma-ipc stalled_client_resyncs`, and the Windows-tier consent tests under `ma-engine`.

## Related Decisions

adr-20260903-automatic-recording-modes, adr-20260903-desktop-stack-and-ipc,
adr-20260903-detector-signal-replay-contract.
