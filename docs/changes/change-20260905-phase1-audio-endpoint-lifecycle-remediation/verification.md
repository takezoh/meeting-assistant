---
change: change-20260905-phase1-audio-endpoint-lifecycle-remediation
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

## Discriminating checks

- A known and active endpoint marked unhealthy appears in both remove and add sets.
- Teardown returns every session registration for the endpoint, removes only its
  known keys, and leaves queued callbacks untouched.
- Active then inactive callbacks remain ordered before the enumerated reconciliation
  view.

## Mechanical gates

- `cargo fmt --all -- --check`
- `cargo test -p ma-signals-windows`
- `cargo test --workspace`
- `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Results (2026-09-05)

- RED then PASS: portable tests require unhealthy same-id remove/add, endpoint-owned
  registration teardown, and ordered notification merge through the helpers used by
  the live path.
- PASS: `cargo test -p ma-signals-windows` (17 tests).
- PASS: `cargo test --workspace`.
- PASS: `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`.
- PASS: native `cargo clippy --workspace --all-targets -- -D warnings`.

`dev-evidence` reports PASS for out-of-scope changes and closure evidence readiness
against base `e3e3c95`. Its `no_test_changes_in_surface` observation is a heuristic
false positive because the portable lifecycle tests are in the scoped Rust source.
