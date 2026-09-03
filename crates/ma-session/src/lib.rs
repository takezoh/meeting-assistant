//! L2 domain crate: the meeting-session lifecycle (contract-session-state-machine), the
//! auto / ask / manual policy with fixed countdown and hysteresis bounds
//! (contract-recording-mode-policy) and the consent-surface precondition
//! (contract-consent-surface-precondition).
//!
//! Everything here is pure. Time enters as [`deadline::Unbiased`] millisecond instants, mode
//! settings and consent surfaces enter as [`state::StepInputs`], and every effect is a returned
//! value in [`state::StepOutcome`]. `step(state, event, now, inputs)` is total: an undeclared
//! `(state, event)` pair returns `Rejected` and changes nothing.

pub mod deadline;
pub mod mode;
pub mod state;
pub mod transition_table;

pub use deadline::{
    DeadlineKind, Deadlines, Unbiased, CANCEL_QUIET_MS, COUNTDOWN_MS, END_HYSTERESIS_MS,
    EXTENSION_MS, PROMPT_MS,
};
pub use mode::{AppClass, MeetingIdentity, Mode, ModeSettings, ResolvedMode};
pub use state::{
    Action, Cause, CauseKind, ClientCapabilities, ConsentSurfaces, Effect, Event, NotifyKind,
    RecoveredState, SessionState, State, StepInputs, StepOutcome, TransitionRecord,
};
pub use transition_table::{transition_table, EventKind, Guard, Transition};
