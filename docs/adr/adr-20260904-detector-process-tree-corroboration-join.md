---
id: adr-20260904-detector-process-tree-corroboration-join
kind: adr
title: The detector joins tab and microphone evidence on the browser process tree
summary: decide() corroborates a browser-class candidate only when its tab and microphone
  signals carry the same process_tree_root_pid, with named rule ids for the absent-key
  and mismatched-key cases, and both producers are assigned.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - The normative same-process-tree clause of the accepted extension-channel ADR becomes
    executable code with a discriminating test, instead of prose no check enforces.
  - A meeting tab in one browser window can no longer be corroborated by an unrelated
    microphone-using call in a different browser process tree, which would have armed
    a recording of the wrong call in Phase 2.
  - The two failure modes are distinguishable in the diagnostics, so a recording that
    did not start can be explained as a missing key rather than a wrong key.
  negative:
  - Corroboration now depends on a field that both producers must populate, so a producer
    that omits it turns a working detection into a permanent Inconclusive rather than
    into a visible error.
  - Resolving a browser process tree root is an operating-system call on the collector
    side and a peer-process lookup on the extension side, both of which can fail and
    both of which then suppress detection.
  - ma-detect is no longer the untouched crate Phase 0 left; its candidate evaluation
    gains state and a rule.
  neutral:
  - The Outcome enum, partition() and decide()'s signature and purity are unchanged;
    the join is a candidate predicate, not a fifth outcome arm.
  - Desktop-class adapters require no tab evidence, so the rule does not apply to
    them and the Phase 0 decisions golden stays byte-identical.
confirmation: cargo test -p ma-detect same_process_tree_mic_and_tab_corroborate (T1),
  mic_use_from_a_different_process_tree_does_not_corroborate (T1), tab_without_a_process_tree_root_is_inconclusive
  (T1), replay_is_byte_identical (T1), forged_extension_signal_does_not_start_capture
  (T1).
tags:
- detection
- browser
- security
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
- {type: implements, target: adr-20260903-extension-localhost-channel-trust}
source_paths:
- crates/ma-detect/src/detector.rs
- crates/ma-signal/src/envelope.rs
- crates/ma-ext-channel/src/server.rs
updated: '2026-09-05'
---

## Context

`adr-20260903-extension-localhost-channel-trust` states normatively that extension signals are
non-authoritative and that "a determinate start additionally requires an operating-system microphone signal
whose subject process belongs to the same browser process tree". It calls this simultaneously the security
property and the robustness property PLAN section 4 asks for.

Nothing implements it. `Payload.process_tree_root_pid` is declared at `crates/ma-signal/src/envelope.rs:100`
with the doc comment "so tab and microphone facts can be joined", and a repository-wide search finds that
declaration and no reader. `decide()` keys candidates by `adapter_id` alone: any `Os`-authority
`MicCaptureStarted` whose `Subject::Process` matches an adapter's `browser_images` sets
`candidate.microphone`, and any `TabMeetingPresent` on a matched host sets `candidate.tab`. The two
enforcements the Phase 1 draft cited — `conformance_violations()` rejecting an adapter that declares
`corroboration.tab` without `corroboration.microphone`, and `partition(true, true, None)` yielding
`Determinate` — neither compares process trees.

The concrete misimplementation the design critique constructed: a Meet tab is open in one Chrome window with no
microphone in use, and a mic-using web call runs in a second Chrome process tree. A plan-conforming detector
emits `Determinate{Start}` for the Meet adapter, and Phase 2 would arm a recording of the wrong call. The
existing `forged_extension_signal_does_not_start_capture` test passes in that state, because it exercises tab
evidence with no microphone at all.

Neither producer populates the field either: the Windows collectors do not exist yet, and `ma-ext-channel`'s
`signals_for` builds `Payload { audible, ..Default::default() }`.

## Decision

**The join.** For an adapter whose `Corroboration` requires both `tab` and `microphone` — the browser class —
the candidate carries the `process_tree_root_pid` of each side, and corroboration is met only when both are
`Some` and equal.

| condition | outcome | `rule_id` |
| --- | --- | --- |
| both keys present and equal, no competing active meeting | `Determinate{Start}` | `start` |
| both keys present and equal, competing active meeting | `Conflicting{LowerPrecedence}` | existing |
| either key absent | `Inconclusive` | `process-tree-root-absent` |
| keys present and unequal | `Inconclusive` | `process-tree-mismatch` |
| no adapter matches the subject | `Unknown` | existing |

The `Outcome` enum, `partition()` and `decide()`'s signature and purity are unchanged. The join is a predicate
on the candidate's evidence, not a fifth outcome arm, and it composes with the existing `resync-no-autostart`
downgrade unchanged. Desktop-class adapters require no tab evidence, so the rule does not apply to them.

**The producers.** The Windows audio-session collector sets `payload.process_tree_root_pid` on
`MicCaptureStarted` and `MicCaptureStopped` for browser processes, from the process-tree lookup it already
performs to attribute the session. `ma-ext-channel`'s `signals_for` copies it from an additive
`Request.peer_process_tree_root_pid` field that the transport supplies; the association between a connection
and its peer exists only inside the server at handle time, because `Server::drain()` returns bare `Signal`s
with no connection identifier. The Windows peer-to-tree-root lookup itself lives in `ma-signals-windows` and is
invoked by the L5 composition root, so `ma-ext-channel` gains no platform dependency.

This ADR supersedes nothing. It discharges an obligation the accepted channel ADR already created.

## Alternatives considered

**Leave `ma-detect` fixed and assert the property in the adapter tables.** The draft's position, restated three
times as "treat `ma-detect` as fixed". Rejected because a declarative `adapter.toml` table describes one
subject at a time and cannot express a relation between two signals; `conformance_violations()` can require
that a browser adapter *declares* it needs both kinds of evidence, but not that the two instances belong to the
same tree.

**A new `SignalKind` carrying the tab and microphone pair.** Would let a producer assert the correlation once.
Rejected because it adds a variant to a closed fifteen-variant enum for a fact two existing signals already
carry, obliges a `schema_version` bump under `NFR-104`, and moves the correlation decision out of the pure
detector into a collector where it is no longer replayable from a fixture.

**Join on `Subject::Process.pid` instead of the tree root.** Simpler and needs no extra field. Rejected because
a browser's tab process and its audio-capturing process are different processes by design; a pid join would
never match, which is the failure the tree-root field was declared to avoid.

**Treat an absent key as corroborating (fail open).** Would keep detection working while the producers are
built. Rejected because it reproduces the defect exactly: a signal set with no keys is the current state, and
defaulting it to corroborating means the join is decorative.

## Consequences

**Positive.**

- The accepted ADR's normative clause becomes code with a discriminating test — a cross-tree fixture that must
  yield `Inconclusive` — instead of prose no check enforces.
- A Meet tab can no longer be corroborated by an unrelated call in a different browser process tree, so Phase 2
  cannot arm a recording of the wrong meeting from this path.
- The two failure modes have distinct rule ids, so a detection that did not fire is explainable in the
  diagnostics as a missing key rather than a wrong key.

**Negative.**

- Corroboration now depends on a field both producers must populate. A producer that omits it turns a working
  detection into a permanent `Inconclusive` rather than a visible error, and only the `process-tree-root-absent`
  rule id in the diagnostics reveals it.
- Resolving a tree root is an operating-system call on the collector side and a peer-process lookup on the
  extension side; both can fail, and both then suppress detection rather than degrading it.
- `ma-detect` is no longer the untouched crate Phase 0 left. Its candidate evaluation gains two fields of state
  and a rule, and every future change to it inherits that.

**Neutral.**

- `Outcome`, `partition()` and `decide()`'s signature and purity are unchanged.
- The rule does not apply to desktop-class adapters, so `desktop-start-end.decisions.json` stays byte-identical
  — which the existing replay test proves rather than assumes.

## Confirmation

`cargo test -p ma-detect same_process_tree_mic_and_tab_corroborate` (T1),
`mic_use_from_a_different_process_tree_does_not_corroborate` (T1),
`tab_without_a_process_tree_root_is_inconclusive` (T1); the existing
`forged_extension_signal_does_not_start_capture` (T1) and `replay_is_byte_identical` (T1) must both stay green.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
