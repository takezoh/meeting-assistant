//! The status panel that opens from the tray icon: the recording, the detected meetings and what can be done
//! with each.
//!
//! `view` decides what is shown and what the panel's messages mean, and is pure; `window` shows it.

pub mod view;
mod window;

pub use self::window::Panel;
