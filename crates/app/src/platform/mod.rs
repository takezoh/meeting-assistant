//! Per-OS implementations. Every OS module exposes types and functions with the same names.
//!
//! Detection and recording:
//! - `PlatformSignalSource`: implements `meeting_assistant_core::SignalSource`
//! - `PlatformRecorder` / `recorder`: implements `meeting_assistant_core::Recorder`, plus the function that creates it
//!
//! Resident app:
//! - `SingleInstance`: prevents multiple instances
//! - `watch_exit_requests`: calls the finalize-recording handler when a logoff or quit request arrives
//! - `request_quit`: asks the resident instance to finalize its recording and quit
//!
//! Installation:
//! - `PlatformInstaller`: implements `crate::install::InstallSystem` (the procedure is decided by `crate::install`)
//!
//! Interaction with the user:
//! - `has_output`: whether a command-line output exists (tries to set one up if not)
//! - `show_error` / `show_info`: reports the result in a dialog when there is no output
//! - `notify`: shows a notification

#[cfg(windows)]
mod win;
#[cfg(windows)]
use self::win as imp;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use self::mac as imp;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("This OS is not supported yet. Add an implementation under crates/app/src/platform.");

pub use self::imp::{
    PlatformInstaller, PlatformRecorder, PlatformSignalSource, SingleInstance, has_output, notify, recorder,
    request_quit, show_error, show_info, watch_exit_requests,
};
