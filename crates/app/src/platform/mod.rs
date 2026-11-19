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
//! Browser link:
//! - `PlatformLoopbackPeers`: implements `crate::browser::peer::LoopbackPeers` (which user owns the other end of a
//!   loopback connection; whether to admit it is decided by `crate::browser::peer`)
//!
//! Installation:
//! - `PlatformInstaller`: implements `crate::install::InstallSystem` (the procedure is decided by `crate::install`)
//!
//! Interaction with the user:
//! - `has_output`: whether a command-line output exists (tries to set one up if not)
//! - `show_error` / `show_info`: reports the result in a dialog when there is no output
//! - `notify`: shows a notification
//! - `panel_window` / `style_panel`: give the status panel's window the platform's look (transparency, material,
//!   rounded corners, accent color)
//! - `panel_shown`: lets the web view hold less memory while the panel is hidden
//! - `tray_look` / `set_tray_icon`: how the tray icon follows the menu bar or taskbar, and where the running time goes
//! - `prepare_event_loop`: what the platform needs before the tray's event loop runs
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
    PlatformInstaller, PlatformLoopbackPeers, PlatformRecorder, PlatformSignalSource, SCREEN_CAPTURE, SingleInstance,
    has_output, notify, panel_shown, panel_window, prepare_event_loop, recorder, request_quit, set_tray_icon,
    show_error, show_info, style_panel, tray_look, watch_exit_requests,
};
