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
//! - `panel_window` / `style_panel`: give the status panel's window the platform's look (transparency, material,
//!   rounded corners)
//! - `SCREEN_CAPTURE`: whether the whole screen can be recorded, so that the UI offers it only where it works

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
    PlatformInstaller, PlatformRecorder, PlatformSignalSource, SCREEN_CAPTURE, SingleInstance, has_output, notify,
    panel_window, recorder, request_quit, show_error, show_info, style_panel, watch_exit_requests,
};
