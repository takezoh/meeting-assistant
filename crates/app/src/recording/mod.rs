//! The parts of recording that do not depend on the OS.
//!
//! Capturing display and audio and writing to the file are done by the per-OS implementations (`platform`).
//! This module holds the parts that are the same for every OS implementation.
//!
//! | Module | Responsibility | Side effects |
//! |---|---|---|
//! | [`plan`] | What to capture and what size to write | none |
//! | [`timeline`] | How write times advance | none |
//! | [`mixer`] | How audio is mixed | none |
//! | [`pipeline`] | How one recording proceeds | none (capture, writing and waiting go through ports) |
//! | [`session`] | Runs a recording on a dedicated thread | threads, clock |

// macOS recording is not implemented yet, so this is unused there. Keep it as is when implementing it.
#![cfg_attr(not(windows), allow(dead_code))]

pub mod mixer;
pub mod pipeline;
pub mod plan;
pub mod session;
pub mod timeline;
