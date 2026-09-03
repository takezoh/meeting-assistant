---
id: adr-20260903-desktop-stack-and-ipc
kind: adr
title: Rust engine with windows-rs, Tauri 2 interface, JSON-RPC over a named pipe
summary: The engine is Rust on windows-rs, the interface is Tauri 2 with WebView2,
  and they speak JSON-RPC 2.0 over a Windows named pipe.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - One language for the engine, the contracts, the boundary tooling and the inference
    bindings, so the boundary check operates over a single graph.
  - The named pipe can be restricted to the owning user at creation, which a loopback
    socket cannot.
  - Tauri keeps the installer small and gives a signed updater that fits the no-backend
    distribution decision.
  negative:
  - WebView2 is a runtime dependency of the interface and its availability is an installation
    concern.
  - The named pipe binds the control channel to Windows; a future macOS phase must
    implement a different transport behind the same protocol.
  - Rust bindings to whisper.cpp and sherpa-onnx bring a C toolchain into the build
    for those crates.
  neutral:
  - JSON-RPC is verbose on the wire, which is irrelevant at command rates and is why
    level events are explicitly droppable.
  - The protocol is transport-agnostic in the crate layout so the transport can be
    replaced without touching the method set.
confirmation: cargo test -p ma-ipc schema_golden_roundtrip (T0) and handshake_major_mismatch_refused
  (T1); cargo xtask boundary (T0) proves the workspace graph is machine-decidable.
tags:
- architecture
- platform
- ipc
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

Phase 0 has to fix the implementation stack before any crate exists, because the crate graph, the boundary rules and the control-channel contract all depend on it. The requirements that constrain the choice: process-loopback audio capture and package identity on Windows 11, secret custody in the Windows Credential Manager, local inference through whisper.cpp and sherpa-onnx, a dependency-direction check that can be automated, and a control channel between two processes owned by the same user.

## Decision

The capture engine is a Rust binary using `windows-rs` for WASAPI process loopback, package identity and the Credential Manager. The user interface is Tauri 2 on WebView2. Local inference uses the whisper.cpp and sherpa-onnx Rust bindings. The dependency-direction check runs over a cargo workspace, so `cargo metadata` is the machine-readable crate graph and `cargo-deny` covers advisories, licences and banned crates.

The control channel is JSON-RPC 2.0 over a Windows named pipe in message mode, one connection per client. Because a named pipe carries no origin concept, transport authorization is a separate obligation fixed by the threat-model ADR: an owner-only discretionary access control list, `FILE_FLAG_FIRST_PIPE_INSTANCE` against name squatting, and a client security-identifier comparison before method dispatch. Because JSON-RPC carries no resynchronization concept, the protocol adds an authoritative snapshot plus a strictly increasing per-connection event sequence, and a client that sees a gap re-snapshots.

## Alternatives considered

**.NET 8 with WinUI 3.** First-class Windows API access and a mature audio stack. Rejected because local inference would cross a native boundary anyway, and because the dependency-direction check has weaker off-the-shelf tooling than a cargo workspace graph.

**Electron with a Rust sidecar.** Familiar interface tooling. Rejected for install size and for adding a Node runtime to a product whose selling point is local-only operation with a small footprint.

**Loopback TCP for the control channel.** Simpler to develop and cross-platform. Rejected because a loopback socket is reachable by every process on the machine and would need its own authentication scheme, where a named pipe can be restricted by access control list at creation.

**Shared-memory ring for the control channel.** Lowest latency. Rejected because the channel carries commands and state transitions at human rates, not audio, and a shared-memory protocol is far harder to version across two independently updatable processes.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- One language for the engine, the contracts, the boundary tooling and the inference bindings, so the boundary check operates over a single graph.
- The named pipe can be restricted to the owning user at creation, which a loopback socket cannot.
- Tauri keeps the installer small and gives a signed updater that fits the no-backend distribution decision.

**Negative.**

- WebView2 is a runtime dependency of the interface and its availability is an installation concern.
- The named pipe binds the control channel to Windows; a future macOS phase must implement a different transport behind the same protocol.
- Rust bindings to whisper.cpp and sherpa-onnx bring a C toolchain into the build for those crates.

**Neutral.**

- JSON-RPC is verbose on the wire, which is irrelevant at command rates and is why level events are explicitly droppable.
- The protocol is transport-agnostic in the crate layout so the transport can be replaced without touching the method set.

## Confirmation

cargo test -p ma-ipc schema_golden_roundtrip (T0) and handshake_major_mismatch_refused (T1); cargo xtask boundary (T0) proves the workspace graph is machine-decidable.


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
