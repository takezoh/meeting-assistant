---
id: adr-20261117-rust-workspace
kind: adr
title: Implement the app as a Rust workspace
status: accepted
created: '2026-11-17'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- Cargo.toml
- crates/core/Cargo.toml
- crates/app/Cargo.toml
summary: Rust workspace with a platform-independent core crate and one executable
  crate; replaces the C# version.
updated: '2026-11-17'
---

# Implement the app as a Rust workspace

## Context

The app must stay resident for the whole logon session, call Windows capture, audio and Media Foundation APIs directly, and later run on macOS with a different set of native APIs. A first version was written in C# on .NET. The user stated that macOS support would follow and asked whether Rust would be the better choice.

A .NET app would need either a different UI and interop stack on macOS or a second implementation. The logic that decides when to record does not depend on any platform and is the part most worth sharing and testing.

## Decision

The app is a Rust workspace with two crates. `crates/core` holds the platform-independent decisions and the ports. `crates/app` builds the single executable and holds the adapters, with operating system code confined to `platform/win` and `platform/mac`. The C# implementation was deleted.

## Consequences

- Positive: one codebase and one binary per platform, no runtime to install, and direct access to the native APIs of each system.
- Positive: the decision logic compiles and is tested without any platform dependency.
- Negative: the Windows COM and WinRT APIs are used through `unsafe` bindings, which need more care than their .NET projections.
- Negative: no cross-platform capture library is in use; each platform's capture must be written against a port.
- Neutral: the macOS target can be type-checked from Windows, which catches compile errors but says nothing about behavior.

## Confirmation

`cargo test --workspace` passes without platform services. `crates/core/Cargo.toml` has no dependency on an OS crate, which `crates/core/tests/architecture.rs` checks. `cargo clippy --workspace --target aarch64-apple-darwin` succeeds.


{% transition from="proposed" to="accepted" date="2026-11-17" %}
Decided by the user on 2026-11-17
{% /transition %}
