---
id: adr-20260903-workflow-runtime-process-topology
kind: adr
title: Workflow runtime in the engine process, native processors in per-job child
  processes
summary: The workflow runtime runs inside the engine; every native or external processor
  runs in a per-job child process; the store has exactly two writer processes.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Processing continues with no window open and stops for no reason other than its
    own failure.
  - A processor crash is confined to a child process and observably cannot change
    session state, which makes the PLAN section 7 guarantee testable rather than asserted.
  - One process owns the session, workflow and export families, so step state and
    session state can be committed in the same transaction.
  - Cancellation of a native job is bounded by construction (terminate the child)
    rather than by processor cooperation.
  negative:
  - The engine process now hosts a scheduler alongside real-time audio, so the capture
    thread's priority is load-bearing rather than incidental.
  - Every native job pays process-spawn latency and needs a framing protocol for requests,
    progress and results.
  - Two store writers means cross-process write-ahead-log contention remains, bounded
    by busy_timeout and verified not to reach capture.
  neutral:
  - The interface gains two control-channel methods it would not otherwise need.
  - The processor host is a third binary to build and sign, though not a long-lived
    process.
confirmation: cargo xtask boundary --rule capture-path-isolation and --rule native-inference-confinement
  (T0); cargo test -p ma-engine --test durability processor_abort_keeps_recording
  (T2) and wedged_writer_does_not_stall_capture (T2).
tags:
- architecture
- process-topology
- workflow
- reliability
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

PLAN section 7 requires that a processing failure never stop the recording path, and PLAN section 9.8 makes it an MVP completion criterion. PLAN section 2 requires that processing continue in the background after a meeting ends. The chosen processors (whisper.cpp, sherpa-onnx) are native libraries that can terminate a process by `abort()` or by an access violation.

Three questions look separate but are one decision, because they change the same writer table, the same process inventory and the same argument for satisfying PLAN section 7: where the workflow runtime lives, whether native processors are isolated, and how many processes write the database. Answering any one alone leaves the others undecidable, so they are recorded here together.

## Decision

The workflow runtime — the durable queue, the scheduler, step lifecycle and the export queue — runs **inside the engine process**.

Every processor invocation that loads a native inference library or executes an external program runs in **`ma-processor-host.exe`, one child process per job**, spawned by the engine's supervisor, bounded by a job object with a 4 GiB memory cap, cancelled by termination after a five-second graceful window, and classified `HostCrashed` on any abnormal exit. A pure-Rust, allocation-bounded processor may run in-process; the manifest declares which, and the boundary check enforces that no in-process processor crate links a native inference library.

The store therefore has exactly **two writer processes**: the engine writes the `session`, `workflow`, `export` and `tombstone` families; the user interface writes the `settings` family. Every other mutation the interface needs goes through a named control-channel method (`artifact.edit`, `meeting.delete`), which keeps the method set small rather than turning the channel into a general remote-write surface.

The isolation guarantee does not rest on the engine binary's dependency edges — a composition root depends on everything by definition. It rests on two boundary rules over the crate graph (the capture-path crates may not reach the workflow, processor or destination crates; only the processor host and its adapters may link native inference libraries) plus the child-process crash boundary. The capture thread additionally runs at pro-audio priority so a loaded scheduler thread cannot starve the audio callback.

## Alternatives considered

**Workflow runtime in the user interface process.** The interface is tray-resident and the queue is durable, so restarts are safe. Rejected because processing stops whenever the user fully exits the application, which contradicts PLAN section 2's background processing, and because a native processor crash would take the interface with it.

**A third dedicated worker process.** Isolates native crashes from both capture and interface and lets processing continue headlessly. Rejected because the per-job child process already provides the crash boundary; a third long-lived process adds a second control channel, a second update unit and a third store writer for no additional property.

**Native processors in-process in the engine.** Minimises processes. Rejected outright: an `abort()` inside whisper.cpp terminates the process that is writing audio, which is precisely the failure PLAN section 7 forbids.

**Routing every database write through the engine (single writer).** Makes the ownership rule trivially true and removes cross-process contention. Rejected because it turns every settings change into a remote call and makes a preferences screen depend on engine availability, to remove contention that write-ahead logging already handles at human write rates.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Processing continues with no window open and stops for no reason other than its own failure.
- A processor crash is confined to a child process and observably cannot change session state, which makes the PLAN section 7 guarantee testable rather than asserted.
- One process owns the session, workflow and export families, so step state and session state can be committed in the same transaction.
- Cancellation of a native job is bounded by construction (terminate the child) rather than by processor cooperation.

**Negative.**

- The engine process now hosts a scheduler alongside real-time audio, so the capture thread's priority is load-bearing rather than incidental.
- Every native job pays process-spawn latency and needs a framing protocol for requests, progress and results.
- Two store writers means cross-process write-ahead-log contention remains, bounded by busy_timeout and verified not to reach capture.

**Neutral.**

- The interface gains two control-channel methods it would not otherwise need.
- The processor host is a third binary to build and sign, though not a long-lived process.

## Confirmation

cargo xtask boundary --rule capture-path-isolation and --rule native-inference-confinement (T0); cargo test -p ma-engine --test durability processor_abort_keeps_recording (T2) and wedged_writer_does_not_stall_capture (T2).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
