# Repository Instructions for Coding Agents

These instructions apply to the entire repository unless a more specific
`AGENTS.md` exists below the file being changed.

## Communication

- Respond in the language used by the user unless they explicitly request another
  language.
- Lead explanatory answers with the conclusion, followed by only the evidence and
  assumptions needed to support it.
- Distinguish verified facts, inference, evaluation, and uncertainty.
- Match the language, format, and level of detail of generated artifacts to their
  intended audience and use.

## Objective and scope

- Reconcile the request, its purpose, constraints, and completion criteria before
  acting.
- Keep changes focused. Do not add adjacent features unless omitting them would make
  the requested result invalid or unsafe.
- If the requested mechanism does not achieve the stated purpose, explain the
  mismatch and use the smallest suitable alternative.
- Do not conceal missing information with assumptions. State what is known, what is
  inferred, and what still requires evidence.

## Project invariants

- Windows 11 is the MVP platform. Linux may run portable checks but is not a product
  target.
- The workflow data path must not depend on a proprietary first-party backend.
- Meeting detection must not depend on DOM structure, content scripts, UI labels,
  screen coordinates, accessibility-tree layout, private APIs, or internal network
  payloads.
- The browser extension remains detection-only. It must not capture tab audio or
  inspect page content.
- An extension signal is non-authoritative and must not start recording by itself.
- Meeting-service-specific identifiers and behavior belong in L4 adapter crates and
  their fixtures, not in workflow or detection core code.
- Preserve capture-path isolation and native-inference confinement as defined in
  `boundary.toml`.
- Keep secrets and meeting content out of logs, general-purpose serialization, and
  command-line arguments. Follow `docs/design/threat-model.md` and
  `docs/design/credential-policy.md`.
- Do not weaken a test, verification registration, security check, or architecture
  rule merely to make a change pass.

## Read before changing code

1. Read `PLAN.md` for product goals, phase boundaries, and non-goals.
2. Read the relevant active document under `docs/design/`.
3. Read the relevant ADRs under `docs/adr/` when the change touches a recorded
   decision.
4. Read the owning package under `docs/changes/` for active or continuing work.
5. Inspect `boundary.toml`, `verification-tiers.toml`, and
   `manual-verification.toml` when changing boundaries or verification coverage.

Code is the primary implementation record. Active design documents govern future
changes; change packages record requirements, implementation history, and evidence.
Keep all three consistent.

## Architecture

The workspace uses six layers:

| Layer | Responsibility |
| --- | --- |
| L0 | Core types |
| L1 | Signals, IPC, processor/destination contracts, manifests, security |
| L2 | Session, detector, workflow |
| L3 | Storage, capture, Windows collectors, extension channel |
| L4 | Meeting-service, processor, and destination adapters |
| L5 | Composition roots, UI, hosts, and repository tools |

Dependencies normally point to lower layers. L4 is a sink and may depend only on
L0/L1; only L5 composition roots may depend on L4. `boundary.toml` is authoritative,
and `cargo xtask boundary` enforces the model over all features and transitive edges.

When adding a crate, assign it to a layer. When moving a responsibility between
layers or changing a trust boundary, update the governing design/ADR instead of
adding an exception.

## Implementation rules

- Keep detector logic deterministic and free of time, filesystem, network, process,
  randomness, and unordered-map dependencies forbidden by `boundary.toml`.
- Put Windows API calls behind `cfg(windows)` and preserve portable fake-backed
  tests for their policy and mapping logic.
- Keep native capture and inference dependencies out of UI and workflow crates.
- Preserve closed schemas. New fields require an explicit contract decision and
  conformance updates.
- Use typed failures for unavailable capabilities, authentication requirements,
  and permanent failures. Do not silently fall back across trust boundaries.
- Preserve explicit user control. Diagnostic capture starts only from an explicit
  operator command.
- Do not commit generated extension endpoint files, build output, credentials,
  captured meeting content, or host identifiers.
- Treat real Windows application/device observations as manual evidence; redact and
  digest-pin records according to `manual-verification.toml`.

## Required verification

Run checks proportional to the changed surface. The normal portable gate is:

```bash
cargo fmt --all -- --check
cargo xtask boundary
cargo xtask verify --check-registration
cargo xtask verify --tier portable --strict
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path app/ui/src-tauri/Cargo.toml --no-default-features --locked
```

On Windows, also run:

```powershell
cargo xtask verify --tier windows --strict
```

For Windows setup and builds, prefer the maintained entry points:

```powershell
.\scripts\windows\dev.ps1 bootstrap
.\scripts\windows\dev.ps1 verify
.\scripts\windows\dev.ps1 build -Configuration Release
.\scripts\windows\dev.ps1 install -Configuration Release
```

Do not report a Windows-native behavior as verified from a Linux build, a GNU
cross-target check, or a fake-backed test. Record any unrun native or manual check
as remaining evidence.

## Documentation and review

- Update the relevant change package when requirements, implementation scope, or
  verification evidence changes.
- Update an active design document only when the standing responsibility, boundary,
  invariant, or policy changes.
- Add or update an ADR when a decision constrains future changes or replaces an
  accepted decision.
- Record out-of-scope defects or deferred work instead of silently folding them into
  the current change.
- Review changes against their declared scope and completion criteria, not only
  against compilation success.
- Before completion, check formatting, tests, documentation consistency, generated
  files, and repository status. Explicitly report unverified behavior and residual
  risk.

## Git hygiene

- Preserve unrelated user changes and untracked files.
- Stage files explicitly; do not use broad staging when unrelated files may exist.
- Do not bypass hooks or use `--no-verify`.
- Do not rewrite published history or force-push unless the user explicitly requests
  it and the target is safe.
- Do not commit credentials, local endpoint configuration, build artifacts, captured
  content, or temporary verification output.
