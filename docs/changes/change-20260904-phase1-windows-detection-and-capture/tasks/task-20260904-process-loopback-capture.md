---
id: task-20260904-process-loopback-capture
kind: task
title: process-loopback-capture-source
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-capture/src/wasapi/mod.rs
- crates/ma-capture/src/wasapi/process_loopback.rs
- crates/ma-capture/src/wasapi/manual_fallback.rs
- crates/ma-capture/Cargo.toml
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Implement a WASAPI-backed CaptureSource behind the existing trait using ActivateAudioInterfaceAsync
  with AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK including process-tree mode, with
  system-loopback fallback, an always-available manual path, and a format pin to the
  durability path's sample rate.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-process-loopback-capture.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Implement a WASAPI-backed CaptureSource behind the existing trait using ActivateAudioInterfaceAsync with AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK including process-tree mode, with system-loopback fallback, an always-available manual path, and a format pin to the durability path's sample rate.

## Execution contract

- Output: Rust modules under crates/ma-capture/src/wasapi implementing CaptureSource, an activation-backend trait with live and fake implementations, and portable-tier fixture tests.
- Tool guidance: Implement strictly behind the existing CaptureSource trait so chunk_writer.rs, consolidate.rs and recovery.rs need no change; do not alter CHUNK_SAMPLES, SAMPLE_RATE, QUEUE_CAP_SAMPLES or the durability write order; surface a mid-session activation loss as SourceEvent::FormatChanged with a new origin, never as silent silence.
- Boundaries: Does not decide which microphone endpoint to open, does not compute the leak measurement, and does not own the committed comparison record or its gate (the manual-verification family does).

## Acceptance

- Given a fake activation backend that succeeds for a target application's PID, the resulting CaptureSource's TrackOrigin reports capture_mode = ProcessLoopback and contamination_risk = None.
- Given a fake activation backend that fails or reports the activation type unavailable, the CaptureSource falls back to system loopback with capture_mode = SystemLoopback and contamination_risk = PossibleOtherApps, and a manual Device-mode CaptureSource remains constructible in the same test process.
- Given a fake backend whose device mix format is 48 kHz stereo, the source resamples to 16 kHz mono before emitting SourceEvent::Samples and its TrackOrigin reports sample_rate = 16000 and channels = 1; given a backend that cannot be resampled, the source returns an activation error instead of opening a track whose origin rate differs from ma_capture::SAMPLE_RATE.
- Given the Windows-tier manual procedure for v-win1-loopback-live-activation, the record states per target application whether single-process activation captured the same audio as process-tree activation, and that record is where the per-application requirement FR-107 asks for is written.
- Given cargo test --workspace and cargo clippy --workspace --all-targets -- -D warnings on a non-Windows host, both pass, because the WASAPI activation backend is behind a cfg(windows) attribute, and the fake backend is the portable implementation of the same trait.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 2d3e1d8da683 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 2d3e1d8da683 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
