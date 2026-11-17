# Development guide

This guide gets a contributor from a checkout to a meaningful local verification and points to the document or source that owns each contract. See the [architecture](../ARCHITECTURE.md) for what exists and [AGENTS.md](../AGENTS.md) for the rules that apply to every change.

## Set up

The workspace uses Rust edition 2024 and needs Rust 1.85 or later. Development happens on Windows 11.

```
cargo build --release
```

To type-check the macOS target from Windows, add it once:

```
rustup target add aarch64-apple-darwin
```

## Verify a code change

```
cargo fmt --all
cargo clippy --workspace --all-targets
cargo clippy --workspace --target aarch64-apple-darwin
cargo test --workspace
```

`cargo test` runs the unit tests and the architecture tests. It needs no meeting app, browser, microphone or display. The [testing reference](technical/testing.md) describes each suite and what it does not cover.

To check the real capture path on the current machine:

```
target\release\meeting-assistant.exe --self-test out.mp4
target\release\meeting-assistant.exe --probe
```

`--self-test` records five seconds of the primary display with the default audio devices. `--probe` prints the processes using the microphone, the windows found and the verdict of each rule; use it when changing rules or signal collection.

An installed copy of the app may already be running and holds the single-instance lock. Stop it with `meeting-assistant --quit` before starting a development build, and pass `--config <path>` so the development build uses its own configuration and log folder.

## Verify a documentation change

Run the dev-docs linter from the repository root and check the relative links of the pages you changed:

```
docs lint
```

`docs` stands for the dev-docs CLI. A documentation check does not establish runtime behavior.

## Where to change a contract

| Concern | Primary document or source |
| --- | --- |
| What the product promises and what it does not | [Product](../PRODUCT.md) |
| First impression and value | [README](../README.md) |
| Installation, extension setup, file locations, commands | [INSTALL](../INSTALL.md), `crates/app/src/install/`, `crates/app/src/cli.rs` |
| Configuration keys, rules and tuning | [Configuration](usage/configuration.md), `crates/core/src/config.rs`, `crates/core/src/rules.rs` |
| Diagnosing a missing or incomplete recording | [Troubleshooting](usage/troubleshooting.md) |
| Dependency direction, purity, ports and fakes | [Effect boundaries design](design/design-effect-boundaries.md), the two architecture tests |
| When a meeting starts and ends; what is recorded for it | [Meeting detection design](design/design-meeting-detection.md), `crates/core/src/{rules,detector,coordinator,engine}.rs` |
| How one recording is captured and written | [Capture pipeline design](design/design-capture-pipeline.md), `crates/app/src/recording/` |
| Extension messages, the relay and the tab recording | [Browser tab recording design](design/design-browser-tab-recording.md), [browser link reference](technical/browser-link.md), `crates/app/src/browser/`, `extension/` |
| Install, update, uninstall, autostart, exit | [Installation design](design/design-installation.md), `crates/app/src/install/` |
| Windows implementations | `crates/app/src/platform/win/` |
| macOS implementations | `crates/app/src/platform/mac/` |
| Test suites | [Testing](technical/testing.md) |
| Decisions and their reasons | [ADRs](adr/) |
| Dated observations | [Notes](note/) |
| Known gaps and unstarted work | [Issues](issue/) |

## Adding a platform operation

1. Add the smallest operation that expresses the need to the relevant port (`SignalSource`, `CaptureBackend`, `InstallSystem`).
2. Implement it in the fake and write the test for the shared procedure that uses it.
3. Implement it under `platform/win` and `platform/mac`. If one platform cannot do it yet, return an error that says so; do not leave the name out, because the architecture test requires both platforms to expose the same names.

## Changing the extension

The extension's files are embedded in the executable by `crates/app/src/bundle.rs` and written out by `--install`. After changing a file under `extension/`, rebuild, reinstall and reload the extension in the browser. A new file must be added to the list in `bundle.rs`; a test fails if the manifest references a file that is not bundled.
