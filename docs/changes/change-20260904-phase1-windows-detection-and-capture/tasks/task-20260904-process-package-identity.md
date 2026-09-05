---
id: task-20260904-process-package-identity
kind: task
title: windows-process-package-collector
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-signals-windows/src/lib.rs
- crates/ma-signals-windows/src/process.rs
- crates/ma-signals-windows/src/package_identity.rs
- crates/ma-signals-windows/Cargo.toml
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Implement the Windows process-lifecycle and package-identity collector inside
  the ma-signals-windows scaffold behind the existing SignalSource seam, emitting
  only the closed Signal/Subject shapes ma-signal already defines.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-process-package-identity.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Implement the Windows process-lifecycle and package-identity collector inside the ma-signals-windows scaffold behind the existing SignalSource seam, emitting only the closed Signal/Subject shapes ma-signal already defines.

## Execution contract

- Output: Rust modules under crates/ma-signals-windows/src implementing SignalSource plus an enumerator trait with a live Windows backend and a portable fake, and portable-tier unit tests.
- Tool guidance: Take every service identifier (process image name, package family name pattern) as constructor input supplied by the composition root from the ma-adapter-* tables; a literal in this crate fails cargo xtask boundary immediately. Declare the windows crate only under [target.'cfg(windows)'.dependencies] and only at the single workspace-pinned version.
- Boundaries: Does not implement audio-session or microphone observation, does not implement capture, does not add a SignalKind or Payload field, and does not read adapter.toml files directly.

## Acceptance

- Given a fake process/package enumerator holding the four target applications' processes, ProcessStarted, ProcessStopped and PackageIdentityObserved are emitted with the expected Subject::Process fields and no other field, and the collector's first emitted signal is CollectorStarted.
- Given a fake enumerator in which a target process is already running when the collector starts, the first ProcessStarted the collector emits for that process carries payload.restart_resync = true.
- Given a fixture where the package-identity query fails transiently versus a process that was never packaged, the emitted package_family_name is None in both cases and only the collector's internal diagnostic distinguishes them.
- Given cargo test --workspace and cargo clippy --workspace --all-targets -- -D warnings on a non-Windows host, both pass with the new module present, because every windows-rs dependency is declared under [target.'cfg(windows)'.dependencies] and every COM call site is behind a cfg(windows) attribute, with the fake enumerator as the portable backend.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 8a6e0a5bce2a on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 8a6e0a5bce2a on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
