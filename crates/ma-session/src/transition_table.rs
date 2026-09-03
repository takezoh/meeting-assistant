//! The declared transition set. `contracts/session/transitions.json` is the single source of
//! truth; this table must equal it (checked by `transition_table_matches_contract_json`).

use crate::state::State;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    DetectorStart,
    DetectorEnd,
    DetectorContinues,
    PolicyEvaluate,
    CommandStart,
    CommandStop,
    CommandPause,
    CommandResume,
    CommandCancel,
    CommandDiscard,
    CommandExtendYes,
    CommandExtendNo,
    TimerCountdown,
    TimerHysteresis,
    TimerPrompt,
    TimerExtension,
    SystemSuspend,
    SystemResume,
    CaptureStarted,
    CaptureFailed,
    FinalizeCompleted,
    FinalizeFailed,
    RecoveryFound,
    RecoveryProceed,
    ConsentLost,
}

impl EventKind {
    pub const ALL: [EventKind; 25] = [
        EventKind::DetectorStart,
        EventKind::DetectorEnd,
        EventKind::DetectorContinues,
        EventKind::PolicyEvaluate,
        EventKind::CommandStart,
        EventKind::CommandStop,
        EventKind::CommandPause,
        EventKind::CommandResume,
        EventKind::CommandCancel,
        EventKind::CommandDiscard,
        EventKind::CommandExtendYes,
        EventKind::CommandExtendNo,
        EventKind::TimerCountdown,
        EventKind::TimerHysteresis,
        EventKind::TimerPrompt,
        EventKind::TimerExtension,
        EventKind::SystemSuspend,
        EventKind::SystemResume,
        EventKind::CaptureStarted,
        EventKind::CaptureFailed,
        EventKind::FinalizeCompleted,
        EventKind::FinalizeFailed,
        EventKind::RecoveryFound,
        EventKind::RecoveryProceed,
        EventKind::ConsentLost,
    ];
}

/// Guards are evaluated against the step inputs; the first declared transition whose guard holds
/// is applied. `None` means unconditional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Guard {
    CancelSuppressed,
    NotCancelSuppressed,
    IdentityGone,
    ModeAutoSurface,
    ModeAutoNoSurface,
    ModeAskSurface,
    ModeAskNoSurface,
    ModeManual,
    ExtensionAvailable,
    ExtensionUsed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub from: State,
    pub to: State,
    pub event: EventKind,
    pub guard: Option<Guard>,
    pub effects: Vec<String>,
}

macro_rules! t {
    ($from:ident -> $to:ident, $event:ident $(, $guard:ident)? ; [$($effect:literal),*]) => {
        Transition {
            from: State::$from,
            to: State::$to,
            event: EventKind::$event,
            guard: None $(.or(Some(Guard::$guard)))?,
            effects: vec![$($effect.to_string()),*],
        }
    };
}

/// The full declared transition set, in evaluation order.
pub fn transition_table() -> Vec<Transition> {
    vec![
        // idle
        t!(Idle -> Idle, DetectorStart, CancelSuppressed; ["record_suppressed_rearm"]),
        t!(Idle -> Candidate, DetectorStart, NotCancelSuppressed; ["create_session", "evaluate_policy"]),
        t!(Idle -> Idle, CommandStop; []),
        t!(Idle -> Idle, SystemSuspend; []),
        t!(Idle -> Idle, SystemResume; []),
        t!(Idle -> Idle, RecoveryFound; []),
        // candidate: policy resolution and ask-mode waiting
        t!(Candidate -> Discarded, PolicyEvaluate, IdentityGone; ["discard_empty_artifacts"]),
        t!(Candidate -> Arming, PolicyEvaluate, ModeAutoSurface; ["notify_countdown", "set_deadline_countdown"]),
        t!(Candidate -> Discarded, PolicyEvaluate, ModeAutoNoSurface; ["record_suppression_no_consent_surface", "discard_empty_artifacts"]),
        t!(Candidate -> Candidate, PolicyEvaluate, ModeAskSurface; ["notify_offer_start"]),
        t!(Candidate -> Discarded, PolicyEvaluate, ModeAskNoSurface; ["record_suppression_no_consent_surface", "discard_empty_artifacts"]),
        t!(Candidate -> Discarded, PolicyEvaluate, ModeManual; ["discard_empty_artifacts"]),
        t!(Candidate -> Discarded, CommandStart, IdentityGone; ["notify_meeting_ended", "discard_empty_artifacts"]),
        t!(Candidate -> Recording, CommandStart; ["start_capture"]),
        t!(Candidate -> Discarded, DetectorEnd; ["discard_empty_artifacts"]),
        t!(Candidate -> Discarded, CommandCancel; ["discard_empty_artifacts", "suppress_rearm"]),
        t!(Candidate -> Candidate, CommandStop; []),
        t!(Candidate -> Candidate, SystemSuspend; []),
        t!(Candidate -> Candidate, SystemResume; ["evaluate_policy"]),
        t!(Candidate -> Idle, RecoveryFound; ["discard_empty_artifacts"]),
        // arming: the visible, cancellable countdown
        t!(Arming -> Recording, TimerCountdown; ["clear_deadline_countdown", "start_capture"]),
        t!(Arming -> Recording, CommandStart; ["clear_deadline_countdown", "start_capture"]),
        t!(Arming -> Discarded, CommandCancel; ["clear_deadline_countdown", "discard_empty_artifacts", "suppress_rearm"]),
        t!(Arming -> Discarded, ConsentLost; ["clear_deadline_countdown", "discard_empty_artifacts"]),
        t!(Arming -> Discarded, DetectorEnd; ["clear_deadline_countdown", "discard_empty_artifacts"]),
        t!(Arming -> Arming, DetectorStart; []),
        t!(Arming -> Arming, SystemSuspend; ["freeze_deadlines"]),
        t!(Arming -> Candidate, SystemResume; ["clear_deadline_countdown", "evaluate_policy"]),
        t!(Arming -> Idle, RecoveryFound; ["discard_empty_artifacts"]),
        // recording
        t!(Recording -> Ending, DetectorEnd; ["set_deadline_hysteresis"]),
        t!(Recording -> Finalizing, CommandStop; ["stop_capture", "finalize"]),
        t!(Recording -> Paused, CommandPause; ["pause_capture"]),
        t!(Recording -> Discarded, CommandDiscard; ["stop_capture", "discard_artifacts"]),
        t!(Recording -> Failed, CaptureFailed; ["record_failure"]),
        t!(Recording -> Recording, CaptureStarted; []),
        t!(Recording -> Recording, ConsentLost; ["record_indicator_unavailable"]),
        t!(Recording -> Recording, SystemSuspend; ["freeze_deadlines"]),
        t!(Recording -> Recording, SystemResume; ["resume_deadlines"]),
        t!(Recording -> Recording, DetectorStart; []),
        t!(Recording -> Recording, DetectorContinues; []),
        t!(Recording -> Recording, CommandStart; []),
        t!(Recording -> Recording, CommandCancel; ["notify_already_recording"]),
        t!(Recording -> Interrupted, RecoveryFound; ["mark_interruption"]),
        // paused
        t!(Paused -> Recording, CommandResume; ["clear_deadline_hysteresis", "resume_capture"]),
        t!(Paused -> Finalizing, CommandStop; ["stop_capture", "finalize"]),
        t!(Paused -> Discarded, CommandDiscard; ["stop_capture", "discard_artifacts"]),
        // a paused session stays paused when the meeting seems to end: capture is never resumed by a
        // signal, only by command_resume; after the hysteresis window it finalizes without a prompt
        t!(Paused -> Paused, DetectorEnd; ["set_deadline_hysteresis"]),
        t!(Paused -> Paused, DetectorContinues; ["clear_deadline_hysteresis"]),
        t!(Paused -> Paused, DetectorStart; ["clear_deadline_hysteresis"]),
        t!(Paused -> Finalizing, TimerHysteresis; ["stop_capture", "finalize"]),
        t!(Paused -> Paused, ConsentLost; ["record_indicator_unavailable"]),
        t!(Paused -> Failed, CaptureFailed; ["record_failure"]),
        t!(Paused -> Paused, SystemSuspend; ["freeze_deadlines"]),
        t!(Paused -> Paused, SystemResume; ["resume_deadlines"]),
        t!(Paused -> Interrupted, RecoveryFound; ["mark_interruption"]),
        // ending: hysteresis, prompt and the single extension
        t!(Ending -> Recording, DetectorContinues; ["clear_deadline_hysteresis", "clear_deadline_prompt", "clear_deadline_extension", "reset_extension"]),
        t!(Ending -> Recording, DetectorStart; ["clear_deadline_hysteresis", "clear_deadline_prompt", "clear_deadline_extension", "reset_extension"]),
        t!(Ending -> Failed, CaptureFailed; ["record_failure"]),
        t!(Ending -> Ending, TimerHysteresis, ExtensionAvailable; ["notify_still_in_meeting", "set_deadline_prompt"]),
        t!(Ending -> Finalizing, TimerHysteresis, ExtensionUsed; ["stop_capture", "finalize"]),
        t!(Ending -> Finalizing, TimerPrompt; ["stop_capture", "finalize"]),
        t!(Ending -> Ending, CommandExtendYes, ExtensionAvailable; ["clear_deadline_prompt", "mark_extension_used", "set_deadline_extension"]),
        t!(Ending -> Finalizing, CommandExtendNo; ["stop_capture", "finalize"]),
        t!(Ending -> Finalizing, TimerExtension; ["stop_capture", "finalize"]),
        t!(Ending -> Finalizing, CommandStop; ["stop_capture", "finalize"]),
        t!(Ending -> Discarded, CommandDiscard; ["stop_capture", "discard_artifacts"]),
        t!(Ending -> Ending, DetectorEnd; []),
        t!(Ending -> Ending, ConsentLost; ["record_indicator_unavailable"]),
        t!(Ending -> Ending, SystemSuspend; ["freeze_deadlines"]),
        t!(Ending -> Ending, SystemResume; ["resume_deadlines"]),
        t!(Ending -> Interrupted, RecoveryFound; ["mark_interruption"]),
        // finalizing and terminal states: commands are idempotent successes
        t!(Finalizing -> Completed, FinalizeCompleted; []),
        t!(Finalizing -> Failed, FinalizeFailed; ["record_failure"]),
        t!(Finalizing -> Finalizing, CommandStop; []),
        t!(Finalizing -> Finalizing, DetectorEnd; []),
        t!(Finalizing -> Finalizing, DetectorContinues; []),
        t!(Finalizing -> Finalizing, RecoveryFound; ["finalize"]),
        // a finished session is memory, not a wall: the next detected meeting starts a new one
        t!(Completed -> Completed, DetectorStart, CancelSuppressed; ["record_suppressed_rearm"]),
        t!(Completed -> Candidate, DetectorStart, NotCancelSuppressed; ["create_session", "evaluate_policy"]),
        t!(Completed -> Completed, CommandStop; []),
        t!(Completed -> Completed, RecoveryFound; []),
        // a finished session is memory, not a wall: the next detected meeting starts a new one
        t!(Discarded -> Discarded, DetectorStart, CancelSuppressed; ["record_suppressed_rearm"]),
        t!(Discarded -> Candidate, DetectorStart, NotCancelSuppressed; ["create_session", "evaluate_policy"]),
        t!(Discarded -> Discarded, CommandStop; []),
        t!(Discarded -> Discarded, RecoveryFound; ["discard_empty_artifacts"]),
        t!(Interrupted -> Finalizing, RecoveryProceed; ["finalize"]),
        t!(Interrupted -> Interrupted, CommandStop; []),
        // a finished session is memory, not a wall: the next detected meeting starts a new one
        t!(Failed -> Failed, DetectorStart, CancelSuppressed; ["record_suppressed_rearm"]),
        t!(Failed -> Candidate, DetectorStart, NotCancelSuppressed; ["create_session", "evaluate_policy"]),
        t!(Failed -> Failed, CommandStop; []),
        t!(Failed -> Failed, RecoveryFound; []),
    ]
}
