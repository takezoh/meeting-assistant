---
id: adr-20260904-windows-audio-signal-observation-apis
kind: adr
title: Windows APIs for process loopback capture and microphone-use observation
summary: Process-tree loopback via ActivateAudioInterfaceAsync with a system-loopback
  fallback, and microphone use observed only from audio-session notifications with
  the consent store as corroboration.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - Both facts Phase 1 needs come from documented, unprivileged, process-attributable
    Windows APIs, so no elevation and no driver is required.
  - CaptureMode::ProcessLoopback is used for the purpose it was declared for instead
    of remaining dead data, and the fallback arm already has its recorded contamination
    value.
  - Naming one emitting source removes the precedence ambiguity that would otherwise
    let two conforming collectors emit different signal sets from the same operating-system
    behaviour.
  negative:
  - Per-application loopback behaviour is unknown until it is measured on real hardware,
    so the activation outcome is a recorded probe result rather than a designed guarantee.
  - The one-second latency bound holds only on the session-manager path; a fact that
    reaches the collector solely through the consent store is up to two seconds old
    and is deliberately not emitted at all.
  - A process that never raises a session-manager notification is invisible to detection
    even when the consent store shows it using the microphone.
  neutral:
  - Windows 11 is the only target platform, so the availability of the process-loopback
    activation type is an assumption the platform choice already makes rather than
    a new one.
  - The collector gains an internal diagnostic counter family (inconclusive-source,
    conflicting-source) that no signal carries.
confirmation: cargo test -p ma-signals-windows mic_use_from_fake_session_manager (T0),
  consent_store_never_emits_a_signal_alone (T0), process_loopback_falls_back_to_system_loopback_on_activation_failure
  (T0); cargo xtask manual-record --id v-win1-mic-use-latency-live --require pass
  and --id v-win1-loopback-live-activation --require pass (T2).
tags:
- windows
- audio
- capture
- detection
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
source_paths:
- PLAN.md
- crates/ma-signals-windows
- crates/ma-capture/src/source.rs
updated: '2026-09-05'
---

## Context

Phase 1 must observe two operating-system facts and capture one audio stream, and PLAN section 6 names all
three: application audio-session observation, microphone-use observation, and process-specific loopback capture
where available. The Phase 0 tree fixes the seams (`SignalSource`, `CaptureSource`) and the closed signal
envelope but names no Windows API; `crates/ma-signals-windows` is an empty scaffold and `CaptureSource`'s own
doc comment says "WASAPI arrives in Phase 1 behind this trait".

Two of the questions are technical rather than product choices, and the conductor dispositioned both under
user-delegated authority on 2026-09-04 ("本当に必要なものだけ確認して、決められるところは決めて"). They are
recorded here together because they are one subsystem: the same COM apartment, the same crate, the same
`cfg(windows)` gate, and the microphone observation is what tells the capture side which process to activate
loopback for.

A third question the disposition did not settle is what happens when the two microphone-use sources disagree.
The design critique found that a two-source fallible decision with no precedence rule and no epistemic
partition lets two conforming collectors emit different signal sets from identical operating-system behaviour,
so the precedence rule is decided here rather than left to the implementation.

## Decision

**Capture.** Activate `ActivateAudioInterfaceAsync` with `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`,
including process-tree mode, for a target application's process. Per-application availability is probed at
runtime and recorded rather than assumed. Activation has exactly three outcomes, all legitimate and all
observable: `Activated` with `capture_mode = ProcessLoopback` and `contamination_risk = None`; `Fallback` to
system (default-endpoint) loopback with `capture_mode = SystemLoopback` and
`contamination_risk = PossibleOtherApps`; and `ManualOnly`, the Device-mode path that stays constructible
independent of either. Every outcome yields a valid `CaptureSource` the existing durability path drives
unchanged.

**Microphone use.** `IAudioSessionManager2` session enumeration plus `IAudioSessionNotification` and
`IAudioSessionEvents` state changes is the **only source that may cause a signal**. The
`CapabilityAccessManager` consent store, polled at one second, corroborates and never emits. Precedence is
total: the session manager decides, and the resulting outcome partition is

- **determinate** — a session-state transition for a matched process emits `MicCaptureStarted` or
  `MicCaptureStopped` within one second;
- **unknown** — neither source reports the process, and nothing is emitted;
- **inconclusive** — a consent-store usage window with no session-manager transition emits nothing and
  increments a named diagnostic counter;
- **conflicting** — a consent-store window still open while the session manager reports `Inactive` or
  `Expired` emits `MicCaptureStopped` from the session manager and counts the conflict;
- **failure** — a total failure to register for notifications is the typed startup failure `MicUseUnavailable`,
  reported on `CollectorStarted`, never a silent degradation to consent-store-only signals.

**Latency.** The one-second bound is a property of the session-manager path only, measured on the Windows
tier. Corroboration lands within two seconds because the secondary source is polled at one second; that number
is recorded rather than claimed as the bound.

## Alternatives considered

**System loopback only, with post-hoc source separation.** PLAN section 4 already accepts system loopback's
contamination, but only as a *browser* fallback when the extension is absent. `CaptureMode::ProcessLoopback`
exists with `contamination_risk = None` for the three desktop applications, and choosing system loopback as the
primary path would leave that variant permanently unused while making every desktop recording carry a
contamination risk it does not have to. Kept as the fallback arm of the same contract rather than discarded.

**Polling `IAudioSessionControl` state without notifications.** Simpler and with one fewer COM interface, but it
gives no documented latency bound without busy-polling, and the bound is what `FR-102` requires.

**Consent store as a co-equal source.** Tempting because it covers applications that use `MediaCapture` without
a visible session, but an `Os`-authority `MicCaptureStarted` derived from a one-second poll would let the
detector raise a determinate start from evidence the primary source never saw, and it would make the stated
latency bound arithmetically unreachable. Rejected in favour of corroboration-only.

## Consequences

**Positive.**

- Both facts come from documented, unprivileged, process-attributable APIs; no elevation, no driver, no hook.
- `CaptureMode::ProcessLoopback` is used for its declared purpose, and the fallback path records its
  contamination honestly instead of silently mixing other applications into a track that claims to be clean.
- One emitting source removes a whole class of divergence between conforming implementations.

**Negative.**

- Per-application loopback behaviour stays unknown until measured on real hardware, so `FR-107`'s record is a
  probe result rather than a designed guarantee, and the measurement needs a Windows machine with all four
  applications installed.
- The one-second bound covers only the primary path; a fact reaching the collector solely through the consent
  store is up to two seconds old and is deliberately not emitted at all.
- An application that uses the microphone without raising a session-manager notification is invisible to
  detection even though the consent store shows it, and the collector's counter is the only place that shows so.

**Neutral.**

- Windows 11 as the only target platform is what makes the activation type safe to assume present; a Windows 10
  compatibility path would reopen this decision.
- The collector gains internal diagnostic counters (inconclusive-source, conflicting-source) that no signal
  carries and that no detector reads.

## Confirmation

`cargo test -p ma-signals-windows mic_use_from_fake_session_manager` (T0),
`consent_store_never_emits_a_signal_alone` (T0),
`cargo test -p ma-capture process_loopback_falls_back_to_system_loopback_on_activation_failure` (T0),
`manual_capture_source_available_independent_of_loopback_outcome` (T0);
`cargo xtask manual-record --id v-win1-mic-use-latency-live --require pass` and
`--id v-win1-loopback-live-activation --require pass` (T2, windows tier).


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
