//! The parts of recording that do not depend on the OS.
//!
//! Capturing display and audio and writing to the file are done by the per-OS implementations (`platform`).
//! This module holds the parts that are the same for every OS implementation.
//!
//! | Module | Responsibility | Side effects |
//! |---|---|---|
//! | [`plan`] | What to capture, which audio tracks to write and what size | none |
//! | [`fit`] | How a picture of another size is placed on the canvas | none |
//! | [`timeline`] | How write times advance | none |
//! | [`mixer`] | How each audio source is buffered | none |
//! | [`pcm`] | Turning captured float audio into the mixer's 16-bit stereo | none |
//! | [`pipeline`] | How one recording proceeds | none (capture, writing and waiting go through ports) |
//! | [`session`] | Runs a recording on a dedicated thread | threads, clock |
//! | [`merge`] | Adding a microphone recorded beside a tab recording to it | none (media and files go through ports) |
//! | [`mp4`] | Adding a track to a fragmented MP4 by rewriting its structure | none (reads through a port) |

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "ScreenCaptureKit fits pictures to the canvas itself")
)]
pub mod fit;
pub mod merge;
pub mod mixer;
pub mod mp4;
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS capture delivers float audio")
)]
pub mod pcm;
pub mod pipeline;
pub mod plan;
pub mod session;
pub mod timeline;
