# Meeting Assistant development guidance

Write all documentation, code comments, identifiers, log and error messages, user-visible text and test names in English. Respond to the user in the language they ask for.

Read [README.md](README.md), [PRODUCT.md](PRODUCT.md), [ARCHITECTURE.md](ARCHITECTURE.md) and [docs/development.md](docs/development.md) before changing behavior; the development guide maps each concern to the document or source that owns its contract. Confirm behavioral claims against the implementation and tests.

## Where things belong

- User pages (README, INSTALL, `docs/usage/`) say what someone can do and expect. Implementation detail belongs in ARCHITECTURE and `docs/technical/`.
- A contract that governs future changes is a design under `docs/design/`. The reason for a decision is an ADR under `docs/adr/`. A dated observation is a note under `docs/note/`. Work that is known but not started, and defects outside the current change, are issues under `docs/issue/`.
- Documents under `docs/` with YAML front matter are managed with the dev-docs `docs` CLI. Create them with `docs create`, change status and relations through the CLI, and run `docs lint` after writing. Do not edit `docs/index/` or `docs/schemas/`.
- Do not turn a past test or manual check into a current product guarantee. State what was observed, where and when.

## Structure rules

- Code that decides is separate from code that acts. Decisions are pure functions or types that depend only on ports (traits). Files, clock, threads, sockets and OS APIs sit behind ports.
- Every port has a real adapter and a fake. Test deciding code with the fake.
- Platform code provides small operations only (read, write, capture). Do not write the same procedure once per operating system; put the procedure on the shared side and add the missing operation to a port.
- `crates/core` must not gain a dependency on the file system, clock, threads or an OS crate. OS crates are used only under `crates/app/src/platform`. `crates/core/tests/architecture.rs` and `crates/app/src/architecture.rs` enforce this; when they fail, fix the structure, not the test.
- Do not preserve a boundary defect to keep a diff small.

## Product rules that code must keep

- In app scope, never widen what is recorded as a fallback: no whole-display picture when the window is missing, no all-output audio when app audio fails.
- A recording that has started must be finalized on every exit path.
- Report a recording as started only after capture is actually running.
- The extension ID, the native-messaging host name and the message types are a wire contract between the app and the extension. Change both sides together.

## Verification

For code changes run:

```
cargo fmt --all
cargo clippy --workspace --all-targets
cargo clippy --workspace --target aarch64-apple-darwin
cargo test --workspace
```

The macOS command type-checks and lints only; it does not establish that anything works on a Mac. Passing tests do not establish that real capture, the tray UI, a real browser or a real meeting service work; say which of those you actually ran.

For documentation-only changes run `docs lint` and check the relative links of the pages you changed.

## Operating on the developer's machine

`--install` and `--uninstall` change the current user's registry, Start Menu and Desktop. Run them only when asked to. An installed copy may be running; stop it with `meeting-assistant --quit` before running another instance, and start the installed copy again afterwards.

Do not read or copy from a hosted repository of this project when asked to work from the local checkout.
