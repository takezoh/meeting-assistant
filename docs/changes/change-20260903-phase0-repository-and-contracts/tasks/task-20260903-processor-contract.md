---
id: task-20260903-processor-contract
kind: task
title: processor-contract
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-processor/Cargo.toml
- crates/ma-processor/src/lib.rs
- crates/ma-processor/src/capability.rs
- crates/ma-processor/src/staging.rs
- crates/ma-processor/src/progress.rs
- crates/ma-processor/src/failure.rs
- contracts/processor/processor-manifest.schema.json
- crates/ma-processor/src/host.rs
- crates/ma-processor-host/Cargo.toml
- crates/ma-processor-host/src/main.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-workflow-core-contract}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the replaceable processing seam including capability declaration, input
  isolation, invocation safety, provenance, progress and the time budget.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-processor-contract.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the replaceable processing seam including capability declaration, input isolation, invocation safety, provenance, progress and the time budget.

## Execution contract

- Output: A Rust crate with a scripted fake processor able to simulate slowness, uncancellable work, growth in per-item cost and budget overrun.
- Tool guidance: Assert the child process command line in tests rather than trusting the construction code; measure cancellation as an interval, not as a flag being set.
- Boundaries: Do not implement whisper.cpp, OpenAI, sherpa-onnx or Claude adapters in this phase; do not accept a shell command as configuration.

## Acceptance

- a hostile configuration value is either type-rejected or passed as a single literal argument, and no shell is ever invoked
- the staging directory contains exactly the declared inputs and is removed after the job
- progress is monotonic, cancellation is observed within the declared bound, and per-item cost does not grow across a 240-item run
- a budget overrun emits a warning and the step still succeeds; a model digest mismatch is a permanent failure
- a processor that loads a native library or runs an external program executes inside ma-processor-host, and a scripted host that aborts yields HostCrashed rather than affecting the engine
- a scripted host that stays alive but emits no progress frame for 150 seconds is killed and the step is Retryable{no_progress} with its completed work items preserved, which is a different outcome from HostCrashed


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 72ed56bd17af with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit 72ed56bd17af with approved mechanical gate
{% /transition %}
