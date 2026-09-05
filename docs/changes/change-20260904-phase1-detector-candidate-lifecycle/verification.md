---
change: change-20260904-phase1-detector-candidate-lifecycle
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

Add counterexample tests for both directions:

- tab(A) + mic(B) + ProcessStopped(A) + mic(A) must not start from stale tab(A);
- tab(A) + mic(B) + MicCaptureStopped(B) + tab(B) must not start from stale mic(B);
- a foreign-tree stop retains both original sides.

Run `cargo test -p ma-detect`, workspace tests, native clippy, and formatting checks.

## Results (2026-09-04)

- PASS: all three mixed-tree teardown counterexamples.
- PASS: `cargo test -p ma-detect` (21 tests).
- PASS: `cargo test --workspace`.
- PASS: `cargo clippy --workspace --all-targets -- -D warnings`.
- PASS: `cargo fmt --all`.
