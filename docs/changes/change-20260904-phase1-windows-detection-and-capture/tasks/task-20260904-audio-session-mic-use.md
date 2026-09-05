---
id: task-20260904-audio-session-mic-use
kind: task
title: windows-audio-session-mic-collector
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-signals-windows/src/audio_session.rs
- crates/ma-signals-windows/src/mic_use.rs
- crates/ma-signals-windows/src/endpoint_observation.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-process-package-identity}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Implement microphone-use and audio-session observation from session-manager
  notifications with consent-store corroboration, and expose the per-process capture
  endpoint as capture-side data outside the signal envelope.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-audio-session-mic-use.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Implement microphone-use and audio-session observation from session-manager notifications with consent-store corroboration, and expose the per-process capture endpoint as capture-side data outside the signal envelope.

## Execution contract

- Output: Rust modules under crates/ma-signals-windows/src implementing SignalSource for audio-session and mic-use signals, a separate endpoint-observation accessor, and fake session-manager and consent-store fixtures.
- Tool guidance: Reuse the same SignalSource seam as the process collector; do not introduce a second collector trait, do not add an endpoint field to Payload or a Device subject to a mic signal, and keep the one-second latency claim attached to the session-manager path only.
- Boundaries: Does not select which microphone endpoint is recorded (mic-endpoint-follow-session consumes the observation), does not open any audio stream, and does not change the detector.

## Acceptance

- Given a fake session-manager source driving session-state transitions, AudioSessionCreated, AudioSessionDestroyed, MicCaptureStarted and MicCaptureStopped are emitted attributed to the correct process subject, and each MicCaptureStarted for a browser process carries payload.process_tree_root_pid resolved from the process-tree lookup the collector already performs.
- Given a fake consent-store source reporting microphone use for a process for which the fake session manager reported nothing, no signal is emitted and the named inconclusive-source diagnostic counter increments; given a consent-store window still open while the session manager reports Expired, MicCaptureStopped is emitted and the conflict counter increments.
- Given a fake session manager reporting a session already Active when the collector starts, the first MicCaptureStarted it emits for that session carries payload.restart_resync = true.
- Given a fake session manager whose notification registration fails, the collector reports the typed MicUseUnavailable startup failure on CollectorStarted and emits no microphone signal from the consent store.
- Given a fake session bound to a non-default capture endpoint, the collector exposes that endpoint identifier through its non-Signal endpoint-observation API and emits no Subject::Device signal for it.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit ba2583ed78c3 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit ba2583ed78c3 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
