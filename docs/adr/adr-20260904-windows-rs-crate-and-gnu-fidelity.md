---
id: adr-20260904-windows-rs-crate-and-gnu-fidelity
kind: adr
title: One pinned windows binding, target-gated, and no GNU cross-check gate
summary: Every Windows-only dependency is declared once at workspace level under a
  cfg(windows) target table with a portable fake behind the same trait, and the x86_64-pc-windows-gnu
  cross-check is not a merge gate.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - The ubuntu portable job keeps compiling, testing and linting the whole workspace
    after Phase 1 lands, so every collector and capture behaviour expressible over
    a fake keeps a portable test.
  - One workspace-level version means two Windows crates cannot drift onto different
    binding versions, which is the failure a per-crate pin invites.
  - Removing a cross-check that the portable runner cannot perform stops the plan
    from merge-blocking on a promise about a toolchain it does not test.
  negative:
  - A compile error that only the real Windows toolchain produces is found on the
    windows job, which runs nightly and on pull requests into main, not on every push.
  - Every Windows call site needs a portable counterpart behind the same trait, which
    is real duplicated surface in two crates.
  neutral:
  - The numeric binding version is an implementation detail no observable depends
    on; the enforced rule is that exactly one version exists and that it lives in
    workspace dependencies.
  - The gnu target and mingw toolchain remain usable locally for a fast compile check;
    they are simply not a gate.
confirmation: cargo test -p xtask windows_only_dependencies_are_target_gated (T0);
  cargo clippy --workspace --all-targets -- -D warnings (T0); cargo test --workspace
  on ubuntu-latest.
tags:
- windows
- toolchain
- verification
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
source_paths:
- Cargo.toml
- .github/workflows/ci.yml
- verification-tiers.toml
updated: '2026-09-04'
---

## Context

`adr-20260903-desktop-stack-and-ipc` names `windows-rs` as the Rust binding without pinning a version, and
`Cargo.lock` carries only transitive `windows-sys` and `windows-link`. Phase 1 is the first phase that calls
Windows APIs directly, from two crates: `ma-signals-windows` (process, package, audio session, microphone) and
`ma-capture` (WASAPI activation and capture).

Implementation and verification happen on a Linux host. The portable CI job runs `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` on `ubuntu-latest` on every push and pull request, and
blocks merge. The design draft did not fix `cfg(windows)` gating for the new code, so two conforming
implementations differed observably: portable CI green versus red.

The draft also proposed `cargo check --target x86_64-pc-windows-gnu` as a portable-tier merge gate under
`NFR-102`, and carried the binding choice as an open, spike-first decision. Three facts make the cross-check
untenable as a gate. Its only evidence, `discovered-linux-cross-compile-target-available`, is recorded as
`status: candidate` and scoped to one development host. The `ubuntu-latest` job installs neither the target nor
a mingw linker. And the `windows-latest` runner's default toolchain is MSVC, so a green GNU cross-check says
nothing about the build that actually ships.

## Decision

**One pin, at workspace level.** The `windows` crate is declared once in `[workspace.dependencies]` and every
Windows-only crate refers to it with `workspace = true`. No crate declares its own version.

**Target-gated.** Every Windows-only dependency is declared under `[target.'cfg(windows)'.dependencies]`, and
every WASAPI or COM call site is behind a `cfg(windows)` attribute. Each such call site has a portable fake
behind the same trait — the fake process/package enumerator, the fake session manager and consent store, the
fake activation backend, the fake ACL applier — so the crate's public API is identical on both hosts and every
behaviour expressible over a fake has a portable test.

**Portable green is the standing gate.** `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` must pass on `ubuntu-latest` after Phase 1 lands, and
`v-win1-windows-code-is-cfg-gated` reads the two crates' manifests to assert the declaration rule mechanically,
because "clippy stayed green" does not distinguish a correctly gated crate from one that happens to compile
today.

**No GNU cross-check gate.** `cargo check --target x86_64-pc-windows-gnu` is not registered as a verification.
The Windows-only compile coverage is the windows job compiling the crates for real.

The numeric binding version is an implementation detail: the observable this decision fixes is that exactly one
version exists and that it lives in workspace dependencies, neither of which varies with the number. Nothing
here waits on a spike.

## Alternatives considered

**Keep the binding choice open as a spike-first decision.** The draft's position. Rejected because the plan may
not carry an open design choice, and because the thing the spike was to determine — a version number — changes
no contract's observable, while the thing that does change one — where the dependency is declared and whether
the portable job stays green — needs no spike at all.

**Register the GNU cross-check conditionally, enabled once the spike closes.** The critique's patch hint.
Rejected because a conditional registration is an open decision wearing a registration's clothes, and because
the check would still be testing a toolchain the shipping build does not use.

**Install the GNU target and a mingw linker in the ubuntu job and keep the gate.** Buys a fast compile signal on
every push. Rejected for Phase 1: it lengthens the merge-blocking job for coverage of a toolchain
(`x86_64-pc-windows-gnu`) that the product does not ship, while the MSVC build it does ship is already compiled
on the windows job. The target remains useful locally and is simply not a gate.

**Per-crate version pins.** Simpler to write. Rejected because two crates linking two versions of the same COM
bindings is a defect class the workspace table exists to prevent.

## Consequences

**Positive.**

- The ubuntu portable job keeps compiling, testing and linting the whole workspace after Phase 1 lands, so the
  fake-backed behaviours keep a fast test.
- Two Windows crates cannot drift onto different binding versions.
- The plan does not merge-block on a check the portable runner cannot perform about a toolchain it does not
  test.

**Negative.**

- A compile error that only the real Windows toolchain produces is found on the windows job, which runs nightly
  and on pull requests into `main`, not on every push. The feedback loop for Windows-only code is a day, not a
  minute.
- Every Windows call site needs a portable counterpart behind the same trait; that is real duplicated surface in
  two crates and a real maintenance cost.

**Neutral.**

- The binding version number is an implementation detail; the enforced rule is single-declaration, not a
  specific number.
- The GNU target and the mingw toolchain stay usable locally for a fast compile check.

## Confirmation

`cargo test -p xtask windows_only_dependencies_are_target_gated` (T0), which reads
`crates/ma-signals-windows/Cargo.toml` and `crates/ma-capture/Cargo.toml` and fails on a Windows dependency
outside a `cfg(windows)` target table or on a version declared outside `[workspace.dependencies]`;
`cargo clippy --workspace --all-targets -- -D warnings` (T0); `cargo test --workspace` on the ubuntu portable
job.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
