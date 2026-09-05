---
id: task-20260904-diagnostic-session-harness
kind: task
title: diagnostic-harness-composition-root
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-engine/Cargo.toml
- crates/ma-engine/src/diagnostic/mod.rs
- crates/ma-engine/src/diagnostic/session.rs
- crates/ma-engine/src/bin/ma-diag.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-audio-session-mic-use}
- {type: dependsOn, target: task-20260904-mic-endpoint-follows-session}
- {type: dependsOn, target: task-20260904-extension-signal-delivery}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Give the Phase 1 composition root and diagnostic harness an owner in the
  existing ma-engine crate — wire the collectors, the capture sources, the extension
  channel and the detector, own live session start/stop/cancel and incremental persistence,
  and expose the label and replay commands.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-diagnostic-session-harness.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Give the Phase 1 composition root and diagnostic harness an owner in the existing ma-engine crate — wire the collectors, the capture sources, the extension channel and the detector, own live session start/stop/cancel and incremental persistence, and expose the label and replay commands.

## Execution contract

- Output: Cargo.toml dependency additions with renamed adapter crates, a diagnostic module and a second binary target ma-diag under crates/ma-engine, plus portable tests driven by fake collectors and fake capture sources.
- Tool guidance: Rename every adapter dependency in Cargo.toml (adapter_a = { package = "ma-adapter-teams" } and so on) so no service identifier token appears in ma-engine source; keep every new dependency compiling on a non-Windows host; do not touch supervisor.rs, the instance lock or ma-session.
- Boundaries: Does not implement collectors, capture sources, detector logic or the extension; does not build or consult ConsentSurfaces, the countdown or the hysteresis path; does not start capture from a detector decision.

## Acceptance

- Given ma-diag invoked with no subcommand or with only a listing subcommand, no collector starts, no CaptureSource is constructed and no file is written under the artifact root; capture starts only under the explicit record subcommand.
- Given a diagnostic session driven by fake collectors, every observed signal is appended to the session's JSONL file before the next signal is read, so that a session terminated by cancel or by dropping the harness retains every signal observed up to that point; the offline replay path is the only caller of SignalTimeline::merge.
- Given a completed session, the harness writes <timeline>.decisions.json from one decide() run over the persisted timeline, and the label subcommand attaches a was_meeting entry for a time range to <timeline>.labels.json in the existing sidecar shape.
- Given the wiring, the harness reads the four ma-adapter-* tables under renamed Cargo.toml dependencies, passes their identifiers to the collectors, passes the audio-session collector's observed endpoint identifier into ma-capture's microphone selection, and resolves the extension listener's peer process to a process-tree root that ma-ext-channel copies into the tab signals' payload.
- Given cargo xtask boundary and cargo clippy --workspace --all-targets -- -D warnings, both pass, because ma-engine is L5 and may depend on every lower layer, and no service identifier appears in ma-engine source because every adapter crate is renamed in Cargo.toml.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 6d14e22eb804 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 6d14e22eb804 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
