//! The port for what is known about particular desktop meeting apps.
//!
//! Apps show their meetings differently, so how a meeting's window is told from the app's other windows cannot be one
//! condition for every app. The detector itself knows no app: it goes by when a window appeared, unless an adapter for
//! the app says what a window is. Adapters are pure; they look at what was read, not at the system.

use std::fmt::Debug;

/// What one of an app's windows is, as far as the app's adapter can tell from the windows read at one time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WindowRole {
    /// Nothing about the window tells; the detector goes by when it appeared.
    #[default]
    Unknown,
    /// The app's main window, which is never a meeting's.
    Main,
    /// A window showing a meeting.
    Meeting,
}

/// Knowledge of one desktop meeting app.
pub trait MeetingApp: Debug + Send + Sync {
    /// Whether this adapter is about the app run as this executable (lowercase, without `.exe`), as a rule's
    /// `process_names` names it.
    fn handles(&self, process_name: &str) -> bool;

    /// What each of the app's windows is, from their titles read at one time, in the same order.
    fn window_roles(&self, titles: &[&str]) -> Vec<WindowRole>;
}
