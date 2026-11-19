# Testing

This reference describes each test suite, what it establishes and what it leaves to manual verification. The commands are in the [development guide](../development.md).

## Suites

| Suite | Location | What it checks |
| --- | --- | --- |
| Core unit tests | `crates/core/src/*.rs` (`naming`), `crates/core/tests/{config,rules,detector,coordinator,engine}.rs` | Rule evaluation, debounce and grace, target selection, retry and recovery, file naming, configuration parsing and validation. The coordinator and engine run against the fakes in `crates/core/src/fakes.rs` |
| Core architecture test | `crates/core/tests/architecture.rs` | Core sources use no file system, network, process, environment, thread, clock or OS crate, and `Cargo.toml` does not depend on one |
| App unit tests | Inline `mod tests` in `crates/app/src/` | Argument parsing, the install procedure against a fake `InstallSystem`, the recording plan, timeline and mixer, the pipeline and threaded recorder against a fake `CaptureBackend`, the extension protocol and hub, which connections the listeners admit, both listeners driven by real clients over loopback with a fake `LoopbackPeers`, the linked signal source and recorder, the worker loop, the configuration file adapter, the bundled extension files |
| App architecture test | `crates/app/src/architecture.rs` | OS crates are used only under `platform`; deciding modules touch no file, socket, process, environment, platform or adapter; time and audio calculations use no clock or thread; both platform implementations expose the same names |
| macOS adapter tests | `crates/app/src/platform/mac/{peers.rs,recorder/mod.rs,recorder/microphone.rs}` | On macOS only. The kernel's TCP connection list names this process's user as the owner of its own loopback connection. AVAssetWriter finishes an MP4 from generated pictures and sound, without any permission. The microphone test is ignored by default because it opens the real microphone and needs its permission for the process running the tests: `cargo test -p meeting-assistant microphone -- --ignored --nocapture` |
| Windows adapter tests | `crates/app/src/platform/win/{peers.rs,recorder/mod.rs,recorder/audio.rs}` | On Windows only. The TCP table and the process token name this process's user as the owner of its own loopback connection. Five recorder tests are ignored by default because they need a graphics adapter, a desktop session or an output device: a captured window of odd size is cropped by one pixel and a resized one scaled with black margins, into NV12, and both are written to an MP4; NV12 pictures are encoded both by the GPU's H.264 encoder and by Windows' software encoder, which shows that an encoder that does not take BGRA is served; 24 pictures written as fast as possible and once a frame are decoded again with every frame in its picture's color, which shows that no texture is drawn into while the encoder holds it, and once a frame they use at most three textures; a window changed every 4 ms is sent as fewer pictures when the capture asks for one per 400 ms than per millisecond; and a tone the test plays is captured from the output device and from its own process. `cargo test -p meeting-assistant -- --ignored --nocapture` |
| Cross-platform type check | `cargo clippy --workspace --target aarch64-apple-darwin` from Windows, `--target x86_64-pc-windows-msvc` from macOS | The other platform's code compiles and passes lints. Nothing is executed. On macOS the whole suite also runs natively |

Production code is scanned by the architecture tests without the test modules at the end of each file (and, in the app, without test-only module declarations such as `mod architecture;`), so tests may use threads and temporary files to drive fakes.

## Conventions

- Test the deciding code through a port's fake. Do not reach for the real adapter in a unit test.
- Time is a value passed in, or a `Pacer`/`Clock` fake. A test must not sleep to make time pass. The threaded-recorder tests are the exception: they cross a real thread and wait on a condition with a deadline.
- A test name states the behavior, for example `cannot_start_a_second_recording_while_recording`.
- When an architecture test fails, the structure is wrong. Move the effect behind a port; do not add the module to an exception list.

## What the tests do not establish

| Not covered by tests | How it was or can be checked |
| --- | --- |
| Windows capture, scaling, audio loopback and the MP4 writer | `meeting-assistant --self-test <path>`, then inspect the file. A real window of odd size, resized while captured, is checked by an ignored test: `cargo test -p meeting-assistant window_pictures -- --ignored` |
| How far sound and picture are apart, and whether that drifts | `tools/avsync` with a long `--self-test <path> <seconds>`; see its [README](../../tools/avsync/README.md) |
| Microphone-session and window enumeration | `meeting-assistant --probe` |
| macOS window, display and app-audio capture with ScreenCaptureKit | `open -n -a "Meeting Assistant" --stdout <log> --args --self-test <path>`, then inspect the file. Started through `open`, macOS asks the permissions for the app; run directly from a terminal, it asks them for the terminal app instead |
| The tray icon, menu, status panel window and page, toasts and dialogs | By hand; the panel's content and messages are tested in `panel/view.rs` |
| The extension in a real browser, the relay, the upload WebSocket from the offscreen document and the browser switch | By hand with Chrome and Edge started from the launcher shortcut |
| Install, update, uninstall and start at logon on a real profile | By hand |
| Finalizing on logoff and shutdown | The message handling was checked by sending the close request; a real logoff was not performed |
| Behavior in real calls on each meeting service | Not done |
| The macOS installer, start at login and the extension path | By hand on one Mac with a throwaway Chrome profile; see [the macOS note](../note/note-20261118-macos-verification-2026-11-18.md). Do not pass `--use-fake-ui-for-media-stream` to a browser used for this: it refuses every tab capture |

Manual results are dated observations, not guarantees. They are recorded in [the verification note](../note/note-20261117-local-verification.md); add a new note for a new round instead of editing an old one.

Do not report a check as passed when it was skipped or not run.
