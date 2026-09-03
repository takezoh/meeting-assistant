---
id: adr-20260903-capture-engine-process-isolation
kind: adr
title: Capture engine runs as a separate per-user single-instance process
summary: The capture engine is its own per-user, single-instance background process
  that owns session truth and outlives every client.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Recording survives the termination of every client, which is a Phase 0 exit criterion
    and is verifiable by killing the interface mid-recording.
  - Detection and automatic recording work with no window open, which is what PLAN
    section 2 asks for.
  - Session truth has exactly one writer, so a client can never disagree with the
    engine about whether a recording is in progress.
  negative:
  - Two long-lived processes must be installed, updated, supervised and debugged,
    and an update has to coordinate them.
  - The control channel becomes a versioned wire contract with two independently updatable
    sides.
  - The engine must be able to raise its own notification, which ties it to package
    identity and to the platform's notification policy.
  neutral:
  - Multi-user machines get one engine per logged-in user; nothing is shared between
    users.
  - A logon-task registration and its repair path become part of installation, which
    no upstream decision named.
confirmation: cargo test -p ma-engine --test topology ui_kill_keeps_recording (T2)
  and second_instance_exits_without_mutation (T2); registered in the windows verification
  tier.
tags:
- architecture
- process-topology
- capture
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

PLAN section 6 names "capture engine as a separate process" as a Phase 0 deliverable, and PLAN section 7 requires that recording continue after the user interface terminates. A single-process design cannot satisfy that: closing a window would end a recording, and a WebView crash would take the audio path with it. PLAN section 2 additionally requires automatic recording to start while the user is not looking at the application, which means detection has to run somewhere that is alive when no window is open.

The repository is greenfield; no prior decision constrains this.

## Decision

The capture engine is `ma-engine.exe`, a background process with one instance per operating-system user. It is started by a logon-registered task and, if not already running, on demand by the user interface. It owns session truth: state transitions, the chunk writer, the artifact directory and the detector all live inside it. Its lifetime is independent of every client — it exits only on an explicit authorized `engine.shutdown`, on user logoff, or on an unrecoverable fault, never because a client disconnected. The user interface is a thin client that renders engine-supplied state and owns no session truth.

Single-instance is enforced by the successful creation of the control pipe with `FILE_FLAG_FIRST_PIPE_INSTANCE`; a second engine that fails to acquire it exits with `EngineAlreadyRunning` without touching any session directory.

Because the engine must be able to start an automatic recording while no client exists, it raises its own operating-system notification as the primary consent surface. An engine that could only speak through a client would be unable to start a recording in exactly the situation this separation exists to serve.

## Alternatives considered

**A single process hosting both the interface and capture.** Simplest to build, install and debug, and it is what most desktop applications do. Rejected because it makes the two PLAN section 7 guarantees unsatisfiable: closing the window ends the recording, and a renderer crash loses audio.

**Engine as a Windows service.** Would survive logoff and start earlier. Rejected because a service runs in session 0 and cannot capture the interactive user's audio endpoints or raise a user-visible notification without a helper, which reintroduces the second process while adding installation privilege requirements.

**Engine supervised by the interface (parent/child).** Rejected because the child dies with the parent by default and the supervision relationship inverts the ownership the design needs.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Recording survives the termination of every client, which is a Phase 0 exit criterion and is verifiable by killing the interface mid-recording.
- Detection and automatic recording work with no window open, which is what PLAN section 2 asks for.
- Session truth has exactly one writer, so a client can never disagree with the engine about whether a recording is in progress.

**Negative.**

- Two long-lived processes must be installed, updated, supervised and debugged, and an update has to coordinate them.
- The control channel becomes a versioned wire contract with two independently updatable sides.
- The engine must be able to raise its own notification, which ties it to package identity and to the platform's notification policy.

**Neutral.**

- Multi-user machines get one engine per logged-in user; nothing is shared between users.
- A logon-task registration and its repair path become part of installation, which no upstream decision named.

## Confirmation

cargo test -p ma-engine --test topology ui_kill_keeps_recording (T2) and second_instance_exits_without_mutation (T2); registered in the windows verification tier.


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
