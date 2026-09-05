---
change: change-20260905-phase1-manual-record-semantic-validation
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

## Discriminating checks

- Reject process identity records whose arrays contain null/empty identifiers,
  whose replay results include false, or whose redaction map has null/empty values.
- Reject loopback maps with null values, keys that disagree across maps, or values
  outside the documented ActivationOutcome, capture mode, and contamination-risk
  domains.
- Reject endpoint selection history containing null, empty, or repeated endpoint
  identifiers.
- Accept concrete examples for all three procedures.

## Mechanical gates

- `cargo fmt --all -- --check`
- `cargo test -p xtask manual_record`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Results (2026-09-05)

- RED then PASS: the three new placeholder counterexamples failed before the
  validator change and passed after it.
- PASS: concrete process identity, loopback activation, and endpoint history
  examples are accepted.
- PASS: `cargo test -p xtask`.
- PASS: `cargo test --workspace`.
- PASS: `cargo clippy --workspace --all-targets -- -D warnings`.

`dev-evidence` reports PASS for out-of-scope changes and closure evidence readiness
against base `9a48391`. Its `no_test_changes_in_surface` observation is a heuristic
false positive: the discriminating unit tests live in the scoped Rust source file.
