//! Adapters of the core's [`MeetingApp`] port: what is known about particular desktop meeting apps.
//!
//! Each adapter is pure and tells, from what was read of an app's windows, which is its main window and which shows a
//! meeting. An app without an adapter is handled by when its windows appeared.

mod slack;

use std::sync::Arc;

use meeting_assistant_core::MeetingApp;

/// Every meeting app adapter.
pub fn adapters() -> Vec<Arc<dyn MeetingApp>> {
    vec![Arc::new(slack::Slack)]
}
