---
id: adr-20260903-automatic-recording-modes
kind: adr
title: Auto, ask and manual modes with a fully numbered timing model and an engine-owned
  consent surface
summary: Three modes with per-application override, a 10 s cancellable countdown,
  60 s cancel suppression and end hysteresis, and a consent surface the engine can
  provide by itself.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Automatic recording works with no window open, which is the behaviour the separate
    engine process was built for.
  - Every timing bound has one number, so each is falsifiable by a test rather than
    always passing.
  - Cancelling is observable on disk, not only in the interface.
  - A suspend across a countdown can never produce a recording the user did not see
    start.
  negative:
  - 'The consent rule now depends on the platform''s notification policy: Focus Assist
    or a revoked permission can suppress automatic recording entirely, which users
    may experience as the feature not working.'
  - Delivery acceptance is not proof of rendering, so a platform that accepts and
    then hides a toast is an accepted residual risk.
  - Sixty seconds of cancel suppression means a user who cancels and immediately changes
    their mind must start manually.
  neutral:
  - A tray-only client is simply one client-kind surface and needs no special status.
  - The chosen numbers are product defaults; changing one is a settings and test change,
    not a design change.
  - Ask mode's Start lives on the same engine notification that carries Cancel in
    auto mode, so no mode requires an attached client; the browser class defaults
    to ask and the window is normally closed.
confirmation: cargo test -p ma-engine --test consent auto_start_with_no_client_attached
  (T2), no_surface_no_capture (T2), cancelled_countdown_writes_no_audio (T2); cargo
  test -p ma-session cancel_suppresses_rearm_for_identity (T1) and suspend_during_countdown_reevaluates
  (T1).
tags:
- ux
- session-lifecycle
- consent
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes three modes (auto with a 10-second countdown cancellable from the notification, ask, manual), per-application override with desktop defaulting to auto and browser to ask, and a 60-second end hysteresis with a "still in the meeting?" notification that can extend. PLAN section 7 requires that recording always be visibly indicated and that users be able to cancel before automatic recording starts.

Three things the decision leaves open change what a user observes, so they are decided here. Every remaining timing bound needs a number, because a bound written as "declared" cannot be falsified by a test. And "who can show the countdown" — and, for ask mode, "who shows Start" — needs an answer that does not defeat the reason the engine is a separate process.

## Decision

Mode resolution order is per-application override, then application-class default (desktop auto, browser ask), then the global setting. All deadlines are evaluated on a clock that excludes system suspend, and on resume every pending deadline is recomputed and every armed decision is re-evaluated against current signals instead of firing, so a laptop closed during a countdown does not wake up recording.

Every bound is a fixed number: a **10-second** countdown; cancellation suppresses re-arming for the same meeting identity until that identity's signals have been continuously absent for **60 seconds**; a determinate end holds the session for **60 seconds**; at expiry a "still in the meeting?" prompt stays answerable for **30 seconds** and grants **one** extension of **300 seconds** per ending episode. Sixty seconds appears twice deliberately: "this meeting is over" is one predicate, used both to clear a cancellation and to finalize.

A **consent surface** is any channel that can show that a recording is about to start and accept a cancel before it does. Two kinds exist. The primary one is the engine's **own operating-system notification**, raised under the application's package identity, with cancel delivered through the engine's own notification activation callback so that no client process participates. The secondary one is an attached client declaring indicator and cancel capabilities. An automatic start requires **at least one**; only the absence of both suppresses it, recorded with its cause. **No mode requires an attached client.** Ask mode does start capture only on an explicit user start, but that start is an action on the engine's own notification, exactly as cancel is; `session.start` from an attached client is an equivalent second path rather than the only one. This is decided rather than left implicit because the browser application class defaults to ask and the window is normally closed, so an ask mode whose only Start affordance lived in a client would make browser meetings unrecordable in the ordinary case — the same inversion rejected above for auto. If no surface of either kind can present the prompt, ask fails closed exactly as auto does.

Symmetrically, if every surface disappears while a session is already recording, capture continues and the engine records that the indicator is unavailable. Starting unobserved is forbidden; continuing unobserved is required.

Finally, no audio sample reaches the artifact root before the recording state. Session metadata and an empty meeting directory may exist during arming, but a pre-roll buffer may live only in memory, so a cancelled countdown is verifiable by inspecting the disk.

## Alternatives considered

**Requiring an attached client as the consent surface.** This was the drafts' rule and it is coherent on its own terms — the engine's independence protects a recording in progress, not the right to start one nobody can see. Rejected because it disables automatic recording in exactly the case the separate engine process exists for: the window is closed, a meeting starts, and nothing happens. The engine notification satisfies the same requirement without the inversion.

**Starting automatically with no indicator at all when none is available.** Rejected: PLAN section 7's visible indication is not satisfiable by a recording nobody can see, so not recording is the correct outcome of the residual case.

**No cancel suppression after a cancel.** Rejected because the next detector tick re-arms immediately and the product asks again every ten seconds.

**Unbounded hysteresis extensions.** Rejected because a user who stops answering would keep a session open indefinitely; one bounded extension makes the ceiling explicit.

**Allowing a disk pre-roll buffer.** Rejected because it makes "cancel before recording starts" untrue on disk while remaining true in the interface.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Automatic recording works with no window open, which is the behaviour the separate engine process was built for.
- Every timing bound has one number, so each is falsifiable by a test rather than always passing.
- Cancelling is observable on disk, not only in the interface.
- A suspend across a countdown can never produce a recording the user did not see start.

**Negative.**

- The consent rule now depends on the platform's notification policy: Focus Assist or a revoked permission can suppress automatic recording entirely, which users may experience as the feature not working.
- Delivery acceptance is not proof of rendering, so a platform that accepts and then hides a toast is an accepted residual risk.
- Sixty seconds of cancel suppression means a user who cancels and immediately changes their mind must start manually.

**Neutral.**

- A tray-only client is simply one client-kind surface and needs no special status.
- The chosen numbers are product defaults; changing one is a settings and test change, not a design change.
- Ask mode's Start lives on the same engine notification that carries Cancel in auto mode, so no mode requires an attached client; the browser class defaults to ask and the window is normally closed.

## Confirmation

cargo test -p ma-engine --test consent auto_start_with_no_client_attached (T2), no_surface_no_capture (T2), cancelled_countdown_writes_no_audio (T2); cargo test -p ma-session cancel_suppresses_rearm_for_identity (T1) and suspend_during_countdown_reevaluates (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
