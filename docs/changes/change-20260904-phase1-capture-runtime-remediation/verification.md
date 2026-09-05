---
change: change-20260904-phase1-capture-runtime-remediation
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

## Discriminating checks

- CRR-001/002: block the writer behind a test seam while the source continues
  producing more than one WASAPI-buffer interval; prove reads continue and any
  overflow is represented by a gap. The pre-change single-worker design must fail.
- CRR-003: script device invalidation through the microphone source and assert
  that the backend receives no system-loopback activation request. The current
  mode-agnostic recovery must fail this check.
- CRR-004: inject append failure independently for process, audio-session, and
  extension batches; assert the next source read never occurs and the command
  returns failure.

## Mechanical gates

- `cargo fmt --all -- --check`
- focused tests for `ma-capture` and `ma-engine`
- `cargo test --workspace`
- `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Results (2026-09-04)

- PASS: focused microphone-loss, stalled-writer, and persistence-failure tests.
- PASS: `cargo test --workspace`.
- PASS: native `cargo clippy --workspace --all-targets -- -D warnings`.
- PASS: `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`.
- PRE-EXISTING FAILURE: Windows-target strict clippy reaches unrelated findings in
  `crates/ma-signals-windows/src/audio_session.rs`; tracked separately as
  `issue-20260904-windows-audio-session-clippy` and not changed here.

The `dev-evidence` out-of-scope binding check passed against base
`8115039d3f44e08373c4656e77fd66130dc7553e`. Its `no_test_changes_in_surface`
observation is a heuristic false positive: the discriminating tests are unit tests
inside the three modified Rust source files rather than separate `tests/` paths.
