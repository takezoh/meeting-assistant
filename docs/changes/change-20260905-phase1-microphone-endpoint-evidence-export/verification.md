---
change: change-20260905-phase1-microphone-endpoint-evidence-export
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

## Discriminating checks

- Convert supplied and system-default endpoint choices into concrete ordered strings.
- Persist and re-read the sidecar, including switch counters.
- Windows target compilation proves the live worker publishes and consumes the same
  record type.

## Mechanical gates

- `cargo fmt --all -- --check`
- `cargo test -p ma-engine --bin ma-diag`
- `cargo test --workspace`
- `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Results (2026-09-05)

- PASS: sidecar conversion/persistence round-trip with supplied and system-default
  choices, ordered history, and switch counters.
- PASS: `cargo test -p ma-engine --bin ma-diag` (15 tests).
- PASS: `cargo test --workspace`.
- PASS: `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`.
- PASS: native `cargo clippy --workspace --all-targets -- -D warnings`.

`dev-evidence` reports PASS for out-of-scope changes and closure evidence readiness
against base `795e20f`. Its `no_test_changes_in_surface` observation is a heuristic
false positive because the sidecar round-trip test is in the scoped Rust source.
