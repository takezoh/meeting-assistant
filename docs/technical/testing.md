# Testing

This reference describes each test suite, what it establishes and what it leaves to manual verification. The commands are in the [development guide](../development.md).

## Suites

| Suite | Location | What it checks |
| --- | --- | --- |
| Core unit tests | `crates/core/src/*.rs` (`naming`), `crates/core/tests/{config,rules,detector,coordinator,engine}.rs` | Rule evaluation, debounce and grace, target selection, retry and recovery, file naming, configuration parsing and validation. The coordinator and engine run against the fakes in `crates/core/src/fakes.rs` |
| Core architecture test | `crates/core/tests/architecture.rs` | Core sources use no file system, network, process, environment, thread, clock or OS crate, and `Cargo.toml` does not depend on one |
| App unit tests | Inline `mod tests` in `crates/app/src/` | Argument parsing, the install procedure against a fake `InstallSystem`, the recording plan, timeline and mixer, the pipeline and threaded recorder against a fake `CaptureBackend`, the extension protocol and hub, the linked signal source and recorder, the worker loop, the configuration file adapter, the bundled extension files |
| App architecture test | `crates/app/src/architecture.rs` | OS crates are used only under `platform`; deciding modules touch no file, socket, process, environment, platform or adapter; time and audio calculations use no clock or thread; both platform implementations expose the same names |
| macOS type check | `cargo clippy --workspace --target aarch64-apple-darwin` | The macOS code compiles and passes lints. Nothing is executed |

Production code is scanned by the architecture tests with everything after `#[cfg(test)]` removed, so tests may use threads and temporary files to drive fakes.

## Conventions

- Test the deciding code through a port's fake. Do not reach for the real adapter in a unit test.
- Time is a value passed in, or a `Pacer`/`Clock` fake. A test must not sleep to make time pass. The threaded-recorder tests are the exception: they cross a real thread and wait on a condition with a deadline.
- A test name states the behavior, for example `cannot_start_a_second_recording_while_recording`.
- When an architecture test fails, the structure is wrong. Move the effect behind a port; do not add the module to an exception list.

## What the tests do not establish

| Not covered by tests | How it was or can be checked |
| --- | --- |
| Windows capture, scaling, audio loopback and the MP4 writer | `meeting-assistant --self-test <path>`, then inspect the file |
| Microphone-session and window enumeration | `meeting-assistant --probe` |
| The tray icon, menu, toasts and dialogs | By hand |
| The extension in a real browser, the relay and the browser switch | By hand with Chrome and Edge started from the launcher shortcut |
| Install, update, uninstall and start at logon on a real profile | By hand |
| Finalizing on logoff and shutdown | The message handling was checked by sending the close request; a real logoff was not performed |
| Behavior in real calls on each meeting service | Not done |
| Anything on macOS | Not done |

Manual results are dated observations, not guarantees. They are recorded in [the verification note](../note/note-20261117-local-verification.md); add a new note for a new round instead of editing an old one.

Do not report a check as passed when it was skipped or not run.
