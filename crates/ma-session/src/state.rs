//! States, events, effects and the total `step` function (contract-session-state-machine).

use crate::deadline::{DeadlineKind, Deadlines, Unbiased, CANCEL_QUIET_MS, COUNTDOWN_MS};
use crate::mode::{AppClass, MeetingIdentity, Mode, ModeSettings, ResolutionSource};
use crate::transition_table::{transition_table, EventKind, Guard};
use ma_core_types::{DecisionId, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Candidate,
    Arming,
    Recording,
    Paused,
    Ending,
    Finalizing,
    Completed,
    Discarded,
    Interrupted,
    Failed,
}

impl State {
    /// Terminal for a session: no capture, no finalization pending. `interrupted` is not terminal
    /// (it proceeds to finalizing on recovery).
    pub fn is_terminal(self) -> bool {
        matches!(self, State::Completed | State::Discarded | State::Failed)
    }

    pub const ALL: [State; 11] = [
        State::Idle,
        State::Candidate,
        State::Arming,
        State::Recording,
        State::Paused,
        State::Ending,
        State::Finalizing,
        State::Completed,
        State::Discarded,
        State::Interrupted,
        State::Failed,
    ];
}

/// What the recovery path found persisted at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveredState {
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    DetectorStart {
        identity: MeetingIdentity,
        class: AppClass,
        decision: DecisionId,
    },
    DetectorEnd {
        decision: DecisionId,
    },
    DetectorContinues {
        decision: DecisionId,
    },
    PolicyEvaluate,
    CommandStart {
        client: String,
    },
    CommandStop {
        client: String,
    },
    CommandPause {
        client: String,
    },
    CommandResume {
        client: String,
    },
    CommandCancel {
        client: String,
    },
    CommandDiscard {
        client: String,
    },
    CommandExtendYes {
        client: String,
    },
    CommandExtendNo {
        client: String,
    },
    TimerCountdown,
    TimerHysteresis,
    TimerPrompt,
    TimerExtension,
    SystemSuspend,
    SystemResume,
    CaptureStarted,
    CaptureFailed {
        reason: String,
    },
    FinalizeCompleted,
    FinalizeFailed {
        reason: String,
    },
    RecoveryFound {
        found: RecoveredState,
    },
    RecoveryProceed,
    ConsentLost,
}

impl Event {
    pub fn kind(&self) -> EventKind {
        match self {
            Event::DetectorStart { .. } => EventKind::DetectorStart,
            Event::DetectorEnd { .. } => EventKind::DetectorEnd,
            Event::DetectorContinues { .. } => EventKind::DetectorContinues,
            Event::PolicyEvaluate => EventKind::PolicyEvaluate,
            Event::CommandStart { .. } => EventKind::CommandStart,
            Event::CommandStop { .. } => EventKind::CommandStop,
            Event::CommandPause { .. } => EventKind::CommandPause,
            Event::CommandResume { .. } => EventKind::CommandResume,
            Event::CommandCancel { .. } => EventKind::CommandCancel,
            Event::CommandDiscard { .. } => EventKind::CommandDiscard,
            Event::CommandExtendYes { .. } => EventKind::CommandExtendYes,
            Event::CommandExtendNo { .. } => EventKind::CommandExtendNo,
            Event::TimerCountdown => EventKind::TimerCountdown,
            Event::TimerHysteresis => EventKind::TimerHysteresis,
            Event::TimerPrompt => EventKind::TimerPrompt,
            Event::TimerExtension => EventKind::TimerExtension,
            Event::SystemSuspend => EventKind::SystemSuspend,
            Event::SystemResume => EventKind::SystemResume,
            Event::CaptureStarted => EventKind::CaptureStarted,
            Event::CaptureFailed { .. } => EventKind::CaptureFailed,
            Event::FinalizeCompleted => EventKind::FinalizeCompleted,
            Event::FinalizeFailed { .. } => EventKind::FinalizeFailed,
            Event::RecoveryFound { .. } => EventKind::RecoveryFound,
            Event::RecoveryProceed => EventKind::RecoveryProceed,
            Event::ConsentLost => EventKind::ConsentLost,
        }
    }

    /// The cause recorded with every accepted transition.
    pub fn cause(&self) -> Cause {
        match self {
            Event::DetectorStart { decision, .. }
            | Event::DetectorEnd { decision }
            | Event::DetectorContinues { decision } => Cause {
                kind: CauseKind::Signal,
                refs: vec![decision.to_string()],
            },
            Event::PolicyEvaluate => Cause {
                kind: CauseKind::Timer,
                refs: vec!["policy.evaluate".into()],
            },
            Event::CommandStart { client }
            | Event::CommandStop { client }
            | Event::CommandPause { client }
            | Event::CommandResume { client }
            | Event::CommandCancel { client }
            | Event::CommandDiscard { client }
            | Event::CommandExtendYes { client }
            | Event::CommandExtendNo { client } => Cause {
                kind: CauseKind::Command,
                refs: vec![format!("{:?}", self.kind()).to_lowercase(), client.clone()],
            },
            Event::TimerCountdown
            | Event::TimerHysteresis
            | Event::TimerPrompt
            | Event::TimerExtension
            | Event::SystemSuspend
            | Event::SystemResume => Cause {
                kind: CauseKind::Timer,
                refs: vec![format!("{:?}", self.kind()).to_lowercase()],
            },
            Event::CaptureStarted
            | Event::CaptureFailed { .. }
            | Event::FinalizeCompleted
            | Event::FinalizeFailed { .. }
            | Event::ConsentLost => Cause {
                kind: CauseKind::Timer,
                refs: vec![format!("{:?}", self.kind()).to_lowercase()],
            },
            Event::RecoveryFound { found } => Cause {
                kind: CauseKind::Recovery,
                refs: vec![format!("found:{:?}", found.state).to_lowercase()],
            },
            Event::RecoveryProceed => Cause {
                kind: CauseKind::Recovery,
                refs: vec!["recovery.proceed".into()],
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CauseKind {
    Signal,
    Command,
    Timer,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cause {
    pub kind: CauseKind,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Cancel,
    Start,
    Stop,
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyKind {
    Countdown { seconds: u64 },
    OfferStart,
    StillInMeeting,
    AlreadyRecording,
    MeetingEnded,
}

/// Effects are returned values; the engine applies them after persisting the transition record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// The mode store was unreadable: the effective mode is manual and the degradation is visible.
    RecordModeDegraded {
        source: ResolutionSource,
        effective: Mode,
    },
    /// A new `ending` episode begins: the one-per-episode extension is available again.
    ResetExtension,
    CreateSession {
        session_id: SessionId,
    },
    EvaluatePolicy,
    Notify {
        kind: NotifyKind,
        actions: Vec<Action>,
    },
    SetDeadline {
        kind: DeadlineKind,
        at: Unbiased,
    },
    ClearDeadline {
        kind: DeadlineKind,
    },
    FreezeDeadlines,
    ResumeDeadlines {
        recomputed: Vec<(DeadlineKind, Unbiased)>,
    },
    StartCapture,
    StopCapture,
    PauseCapture,
    ResumeCapture,
    Finalize,
    DiscardEmptyArtifacts,
    DiscardArtifacts,
    SuppressRearm {
        identity: MeetingIdentity,
        quiet_ms: u64,
    },
    RecordSuppression {
        cause: String,
    },
    RecordIndicatorUnavailable,
    RecordFailure {
        reason: String,
    },
    MarkInterruption,
    MarkExtensionUsed,
}

/// The persisted record of one accepted transition; appended before effects are applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRecord {
    pub session_id: Option<SessionId>,
    pub from: State,
    pub to: State,
    pub event: EventKind,
    pub guard: Option<Guard>,
    pub cause: Cause,
    pub at: Unbiased,
}

/// Capabilities a client declared at handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub client: String,
    pub indicator: bool,
    pub cancel: bool,
}

/// The consent surfaces available at the moment of a decision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentSurfaces {
    pub notification_deliverable: bool,
    pub clients: Vec<ClientCapabilities>,
}

impl ConsentSurfaces {
    /// The engine notification alone is sufficient; an attached client must declare both
    /// `indicator` and `cancel`. No mode requires an attached client.
    pub fn available(&self) -> bool {
        self.notification_deliverable || self.clients.iter().any(|c| c.indicator && c.cancel)
    }
}

/// Operational inputs acquired per `step` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepInputs {
    pub mode_settings: ModeSettings,
    pub consent: ConsentSurfaces,
    /// When the current meeting identity's signals were last observed; `None` = not observed.
    pub identity_last_seen: Option<Unbiased>,
    /// Fresh identifier minted by the engine for a session created in this step.
    pub next_session_id: SessionId,
}

/// The machine state: the current session (if any) plus the memory that outlives sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub state: State,
    pub session_id: Option<SessionId>,
    pub identity: Option<MeetingIdentity>,
    pub class: Option<AppClass>,
    pub mode: Option<Mode>,
    pub deadlines: Deadlines,
    pub extension_used: bool,
    /// Meeting identities cancelled by the user, with the cancel instant.
    #[serde(with = "cancelled_pairs")]
    pub cancelled: BTreeMap<MeetingIdentity, Unbiased>,
}

/// `cancelled` keyed by a struct cannot be a JSON object; it travels as a list of pairs so a
/// snapshot serializes after a cancel (contract-ipc-protocol: the snapshot is authoritative).
mod cancelled_pairs {
    use super::*;
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(
        map: &BTreeMap<MeetingIdentity, Unbiased>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(&MeetingIdentity, &Unbiased)> = map.iter().collect();
        pairs.serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<MeetingIdentity, Unbiased>, D::Error> {
        let pairs: Vec<(MeetingIdentity, Unbiased)> = Vec::deserialize(d)?;
        Ok(pairs.into_iter().collect())
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            state: State::Idle,
            session_id: None,
            identity: None,
            class: None,
            mode: None,
            deadlines: Deadlines::default(),
            extension_used: false,
            cancelled: BTreeMap::new(),
        }
    }
}

impl SessionState {
    pub fn idle() -> Self {
        Self::default()
    }

    /// A machine restored in `state` for recovery evaluation.
    pub fn restored(state: State, session_id: SessionId) -> Self {
        Self {
            state,
            session_id: Some(session_id),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum StepOutcome {
    Accepted {
        next: SessionState,
        record: TransitionRecord,
        effects: Vec<Effect>,
    },
    Rejected {
        state: State,
        event: EventKind,
    },
}

impl StepOutcome {
    pub fn next_state(&self) -> State {
        match self {
            StepOutcome::Accepted { next, .. } => next.state,
            StepOutcome::Rejected { state, .. } => *state,
        }
    }
    pub fn effects(&self) -> &[Effect] {
        match self {
            StepOutcome::Accepted { effects, .. } => effects,
            StepOutcome::Rejected { .. } => &[],
        }
    }
    pub fn into_next(self) -> Option<SessionState> {
        match self {
            StepOutcome::Accepted { next, .. } => Some(next),
            StepOutcome::Rejected { .. } => None,
        }
    }
}

/// Whether a cancelled identity is still inside its quiet period: suppression holds until the
/// identity's signals have been continuously absent for `CANCEL_QUIET_MS`.
fn cancel_suppressed(
    machine: &SessionState,
    identity: &MeetingIdentity,
    now: Unbiased,
    last_seen: Option<Unbiased>,
) -> bool {
    match (machine.cancelled.get(identity), last_seen) {
        (None, _) => false,
        (Some(_), None) => false,
        (Some(_), Some(seen)) => now.ms_since(seen) < CANCEL_QUIET_MS,
    }
}

fn guard_holds(
    guard: Guard,
    machine: &SessionState,
    event: &Event,
    now: Unbiased,
    inputs: &StepInputs,
) -> bool {
    let identity_live = inputs
        .identity_last_seen
        .is_some_and(|seen| now.ms_since(seen) < CANCEL_QUIET_MS);
    match guard {
        Guard::CancelSuppressed | Guard::NotCancelSuppressed => {
            let suppressed = match event {
                Event::DetectorStart { identity, .. } => cancel_suppressed(
                    machine,
                    identity,
                    now,
                    inputs.identity_last_seen.or(Some(now)),
                ),
                _ => false,
            };
            (guard == Guard::CancelSuppressed) == suppressed
        }
        Guard::IdentityGone => !identity_live,
        Guard::ModeAutoSurface
        | Guard::ModeAutoNoSurface
        | Guard::ModeAskSurface
        | Guard::ModeAskNoSurface
        | Guard::ModeManual => {
            let mode = machine.mode.unwrap_or(Mode::Manual);
            let surface = inputs.consent.available();
            match guard {
                Guard::ModeAutoSurface => mode == Mode::Auto && surface,
                Guard::ModeAutoNoSurface => mode == Mode::Auto && !surface,
                Guard::ModeAskSurface => mode == Mode::Ask && surface,
                Guard::ModeAskNoSurface => mode == Mode::Ask && !surface,
                _ => mode == Mode::Manual,
            }
        }
        Guard::ExtensionAvailable => !machine.extension_used,
        Guard::ExtensionUsed => machine.extension_used,
    }
}

/// The total step function: find the first declared transition for `(state, event)` whose guard
/// holds, build its effects, and return the next machine state with the transition record.
/// Undeclared pairs return `Rejected` and change nothing.
pub fn step(
    machine: &SessionState,
    event: &Event,
    now: Unbiased,
    inputs: &StepInputs,
) -> StepOutcome {
    let kind = event.kind();
    let table = transition_table();
    let Some(transition) = table
        .iter()
        .filter(|t| t.from == machine.state && t.event == kind)
        .find(|t| {
            t.guard
                .is_none_or(|g| guard_holds(g, machine, event, now, inputs))
        })
    else {
        return StepOutcome::Rejected {
            state: machine.state,
            event: kind,
        };
    };
    let mut next = machine.clone();
    next.state = transition.to;
    let mut effects = Vec::new();
    for name in &transition.effects {
        match name.as_str() {
            "create_session" => {
                let Event::DetectorStart {
                    identity, class, ..
                } = event
                else {
                    unreachable!("create_session is only declared for detector_start")
                };
                next.session_id = Some(inputs.next_session_id);
                next.identity = Some(identity.clone());
                next.class = Some(*class);
                let resolved = inputs.mode_settings.resolve(&identity.adapter_id, *class);
                next.mode = Some(resolved.mode);
                if resolved.degraded {
                    effects.push(Effect::RecordModeDegraded {
                        source: resolved.source,
                        effective: resolved.mode,
                    });
                }
                next.extension_used = false;
                next.deadlines.clear_all();
                effects.push(Effect::CreateSession {
                    session_id: inputs.next_session_id,
                });
            }
            "evaluate_policy" => effects.push(Effect::EvaluatePolicy),
            "notify_countdown" => effects.push(Effect::Notify {
                kind: NotifyKind::Countdown {
                    seconds: COUNTDOWN_MS / 1_000,
                },
                actions: vec![Action::Cancel],
            }),
            "notify_offer_start" => effects.push(Effect::Notify {
                kind: NotifyKind::OfferStart,
                actions: vec![Action::Start],
            }),
            "notify_still_in_meeting" => effects.push(Effect::Notify {
                kind: NotifyKind::StillInMeeting,
                actions: vec![Action::Yes, Action::No],
            }),
            "notify_already_recording" => effects.push(Effect::Notify {
                kind: NotifyKind::AlreadyRecording,
                actions: vec![Action::Stop],
            }),
            "notify_meeting_ended" => effects.push(Effect::Notify {
                kind: NotifyKind::MeetingEnded,
                actions: vec![],
            }),
            "set_deadline_countdown"
            | "set_deadline_hysteresis"
            | "set_deadline_prompt"
            | "set_deadline_extension" => {
                let deadline = match name.as_str() {
                    "set_deadline_countdown" => DeadlineKind::Countdown,
                    "set_deadline_hysteresis" => DeadlineKind::Hysteresis,
                    "set_deadline_prompt" => DeadlineKind::Prompt,
                    _ => DeadlineKind::Extension,
                };
                let at = next.deadlines.set(deadline, now);
                effects.push(Effect::SetDeadline { kind: deadline, at });
            }
            "clear_deadline_countdown"
            | "clear_deadline_hysteresis"
            | "clear_deadline_prompt"
            | "clear_deadline_extension" => {
                let deadline = match name.as_str() {
                    "clear_deadline_countdown" => DeadlineKind::Countdown,
                    "clear_deadline_hysteresis" => DeadlineKind::Hysteresis,
                    "clear_deadline_prompt" => DeadlineKind::Prompt,
                    _ => DeadlineKind::Extension,
                };
                next.deadlines.clear(deadline);
                effects.push(Effect::ClearDeadline { kind: deadline });
            }
            "freeze_deadlines" => {
                next.deadlines.suspend(now);
                effects.push(Effect::FreezeDeadlines);
            }
            "resume_deadlines" => {
                let recomputed = next.deadlines.resume(now);
                effects.push(Effect::ResumeDeadlines { recomputed });
            }
            "start_capture" => effects.push(Effect::StartCapture),
            "stop_capture" => effects.push(Effect::StopCapture),
            "pause_capture" => effects.push(Effect::PauseCapture),
            "resume_capture" => effects.push(Effect::ResumeCapture),
            "finalize" => effects.push(Effect::Finalize),
            "discard_empty_artifacts" => effects.push(Effect::DiscardEmptyArtifacts),
            "discard_artifacts" => effects.push(Effect::DiscardArtifacts),
            "suppress_rearm" => {
                if let Some(identity) = &next.identity {
                    next.cancelled.insert(identity.clone(), now);
                    effects.push(Effect::SuppressRearm {
                        identity: identity.clone(),
                        quiet_ms: CANCEL_QUIET_MS,
                    });
                }
            }
            "record_suppressed_rearm" => effects.push(Effect::RecordSuppression {
                cause: "cancel_quiet_period".into(),
            }),
            "record_suppression_no_consent_surface" => effects.push(Effect::RecordSuppression {
                cause: "no_consent_surface".into(),
            }),
            "record_indicator_unavailable" => effects.push(Effect::RecordIndicatorUnavailable),
            "record_failure" => {
                let reason = match event {
                    Event::CaptureFailed { reason } | Event::FinalizeFailed { reason } => {
                        reason.clone()
                    }
                    _ => format!("{:?}", kind).to_lowercase(),
                };
                effects.push(Effect::RecordFailure { reason });
            }
            "mark_interruption" => effects.push(Effect::MarkInterruption),
            "mark_extension_used" => {
                next.extension_used = true;
                effects.push(Effect::MarkExtensionUsed);
            }
            "reset_extension" => {
                next.extension_used = false;
                effects.push(Effect::ResetExtension);
            }
            other => unreachable!("effect {other} is declared in the table but has no constructor"),
        }
    }
    // identities whose quiet period has elapsed are forgotten so the table stays bounded
    if let Event::DetectorStart { identity, .. } = event {
        if next.state == State::Candidate {
            next.cancelled.remove(identity);
        }
    }
    let record = TransitionRecord {
        session_id: next.session_id,
        from: machine.state,
        to: transition.to,
        event: kind,
        guard: transition.guard,
        cause: event.cause(),
        at: now,
    };
    StepOutcome::Accepted {
        next,
        record,
        effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core_types::id::TypedId;
    use std::path::Path;

    fn identity() -> MeetingIdentity {
        MeetingIdentity {
            adapter_id: "adapter-a".into(),
            subject_key: "pid:4242".into(),
        }
    }
    fn decision() -> DecisionId {
        DecisionId::new()
    }
    fn inputs(mode: Mode, notification: bool, last_seen: Option<u64>) -> StepInputs {
        let mut settings = ModeSettings {
            global: mode,
            ..Default::default()
        };
        settings.class_defaults.clear();
        StepInputs {
            mode_settings: settings,
            consent: ConsentSurfaces {
                notification_deliverable: notification,
                clients: vec![],
            },
            identity_last_seen: last_seen.map(Unbiased),
            next_session_id: SessionId::new(),
        }
    }
    fn start(machine: &SessionState, now: u64, inputs: &StepInputs) -> SessionState {
        let out = step(
            machine,
            &Event::DetectorStart {
                identity: identity(),
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(now),
            inputs,
        );
        out.into_next()
            .expect("detector start is declared from idle")
    }
    fn apply(
        machine: &SessionState,
        event: Event,
        now: u64,
        inputs: &StepInputs,
    ) -> (SessionState, Vec<Effect>) {
        match step(machine, &event, Unbiased(now), inputs) {
            StepOutcome::Accepted { next, effects, .. } => (next, effects),
            StepOutcome::Rejected { state, event } => {
                panic!("unexpected rejection of {event:?} in {state:?}")
            }
        }
    }

    #[test]
    fn transition_table_matches_contract_json() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/session/transitions.json");
        let exported = serde_json::to_value(transition_table()).unwrap();
        if std::env::var_os("UPDATE_TRANSITIONS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&exported).unwrap() + "\n",
            )
            .unwrap();
        }
        let declared: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("contracts/session/transitions.json exists"),
        )
        .unwrap();
        assert_eq!(
            exported, declared,
            "code table and contracts/session/transitions.json must agree"
        );
    }

    #[test]
    fn step_is_total_over_state_event_space() {
        let table = transition_table();
        let inputs_variants = [
            inputs(Mode::Auto, true, Some(0)),
            inputs(Mode::Ask, false, None),
            inputs(Mode::Manual, true, Some(0)),
        ];
        let events: Vec<Event> = vec![
            Event::DetectorStart {
                identity: identity(),
                class: AppClass::Browser,
                decision: decision(),
            },
            Event::DetectorEnd {
                decision: decision(),
            },
            Event::DetectorContinues {
                decision: decision(),
            },
            Event::PolicyEvaluate,
            Event::CommandStart { client: "c".into() },
            Event::CommandStop { client: "c".into() },
            Event::CommandPause { client: "c".into() },
            Event::CommandResume { client: "c".into() },
            Event::CommandCancel { client: "c".into() },
            Event::CommandDiscard { client: "c".into() },
            Event::CommandExtendYes { client: "c".into() },
            Event::CommandExtendNo { client: "c".into() },
            Event::TimerCountdown,
            Event::TimerHysteresis,
            Event::TimerPrompt,
            Event::TimerExtension,
            Event::SystemSuspend,
            Event::SystemResume,
            Event::CaptureStarted,
            Event::CaptureFailed { reason: "x".into() },
            Event::FinalizeCompleted,
            Event::FinalizeFailed { reason: "x".into() },
            Event::RecoveryFound {
                found: RecoveredState {
                    state: State::Arming,
                },
            },
            Event::RecoveryProceed,
            Event::ConsentLost,
        ];
        assert_eq!(events.len(), EventKind::ALL.len());
        let mut accepted = 0;
        for state in State::ALL {
            for extension_used in [false, true] {
                for event in &events {
                    for inputs in &inputs_variants {
                        let mut machine = SessionState::restored(state, SessionId::new());
                        machine.mode = Some(inputs.mode_settings.global);
                        machine.extension_used = extension_used;
                        machine.identity = Some(identity());
                        match step(&machine, event, Unbiased(5_000), inputs) {
                            StepOutcome::Accepted { record, next, .. } => {
                                accepted += 1;
                                assert!(table.iter().any(|t| t.from == state && t.to == next.state && t.event == event.kind() && t.guard == record.guard), "accepted transition must be declared: {state:?} --{:?}--> {:?}", event.kind(), next.state);
                                assert_eq!(record.from, state);
                            }
                            StepOutcome::Rejected { state: s, event: e } => {
                                assert_eq!(s, state);
                                assert_eq!(e, event.kind());
                            }
                        }
                    }
                }
            }
        }
        assert!(accepted > 0);
    }

    #[test]
    fn repeated_stop_is_success_without_effect() {
        for state in [State::Finalizing, State::Completed, State::Discarded] {
            let machine = SessionState::restored(state, SessionId::new());
            let inputs = inputs(Mode::Auto, true, None);
            let first = step(
                &machine,
                &Event::CommandStop {
                    client: "ui-1".into(),
                },
                Unbiased(1),
                &inputs,
            );
            let second = step(
                &machine,
                &Event::CommandStop {
                    client: "ui-2".into(),
                },
                Unbiased(1),
                &inputs,
            );
            for out in [&first, &second] {
                assert!(
                    matches!(out, StepOutcome::Accepted { .. }),
                    "redundant stop in {state:?} must not be rejected"
                );
                assert_eq!(out.next_state(), state);
                assert!(
                    out.effects().is_empty(),
                    "no effect for a redundant stop in {state:?}"
                );
            }
        }
        // a genuine stop while recording finalizes exactly once
        let machine = SessionState::restored(State::Recording, SessionId::new());
        let (next, effects) = apply(
            &machine,
            Event::CommandStop {
                client: "ui-1".into(),
            },
            1,
            &inputs(Mode::Auto, true, None),
        );
        assert_eq!(next.state, State::Finalizing);
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::Finalize))
                .count(),
            1
        );
        let (again, effects_again) = apply(
            &next,
            Event::CommandStop {
                client: "ui-2".into(),
            },
            2,
            &inputs(Mode::Auto, true, None),
        );
        assert_eq!(again.state, State::Finalizing);
        assert!(effects_again.is_empty());
    }

    #[test]
    fn recovery_from_arming_lands_in_idle() {
        let machine = SessionState::restored(State::Arming, SessionId::new());
        let inputs = inputs(Mode::Auto, true, Some(0));
        let (next, effects) = apply(
            &machine,
            Event::RecoveryFound {
                found: RecoveredState {
                    state: State::Arming,
                },
            },
            10,
            &inputs,
        );
        assert_eq!(
            next.state,
            State::Idle,
            "an armed countdown is not resumed across a restart"
        );
        assert!(effects.contains(&Effect::DiscardEmptyArtifacts));
        assert!(!effects.iter().any(|e| matches!(e, Effect::StartCapture)));
        // re-arming needs a fresh decision and a fresh countdown
        let fresh = start(&next, 20, &inputs);
        assert_eq!(fresh.state, State::Candidate);
        let (armed, effects) = apply(&fresh, Event::PolicyEvaluate, 20, &inputs);
        assert_eq!(armed.state, State::Arming);
        assert!(effects.contains(&Effect::SetDeadline {
            kind: DeadlineKind::Countdown,
            at: Unbiased(20 + COUNTDOWN_MS)
        }));
        // a session found recording is interrupted and then finalized, keeping its id
        let recording_id = SessionId::new();
        let machine = SessionState::restored(State::Recording, recording_id);
        let (interrupted, effects) = apply(
            &machine,
            Event::RecoveryFound {
                found: RecoveredState {
                    state: State::Recording,
                },
            },
            10,
            &inputs,
        );
        assert_eq!(interrupted.state, State::Interrupted);
        assert!(effects.contains(&Effect::MarkInterruption));
        let (finalizing, effects) = apply(&interrupted, Event::RecoveryProceed, 11, &inputs);
        assert_eq!(finalizing.state, State::Finalizing);
        assert_eq!(finalizing.session_id, Some(recording_id));
        assert!(effects.contains(&Effect::Finalize));
    }

    #[test]
    fn mode_resolution_override_class_global() {
        let mut settings = ModeSettings {
            global: Mode::Ask,
            ..Default::default()
        };
        settings.overrides.insert("adapter-a".into(), Mode::Manual);
        assert_eq!(
            settings.resolve("adapter-a", AppClass::Desktop).mode,
            Mode::Manual,
            "override wins"
        );
        assert_eq!(
            settings.resolve("adapter-b", AppClass::Desktop).mode,
            Mode::Auto,
            "class default wins over global"
        );
        assert_eq!(
            settings.resolve("adapter-b", AppClass::Browser).mode,
            Mode::Ask
        );
        settings.class_defaults.clear();
        assert_eq!(
            settings.resolve("adapter-b", AppClass::Browser).mode,
            Mode::Ask,
            "global is the last resort"
        );
        let unreadable = ModeSettings {
            readable: false,
            ..ModeSettings::default()
        };
        let resolved = unreadable.resolve("adapter-b", AppClass::Desktop);
        assert_eq!(resolved.mode, Mode::Manual);
        assert!(resolved.degraded);
        // a detection resolved to ask returns a notify effect carrying start: satisfiable with no client
        let ask = inputs(Mode::Ask, true, Some(0));
        let candidate = start(&SessionState::idle(), 0, &ask);
        assert_eq!(candidate.mode, Some(Mode::Ask));
        let (waiting, effects) = apply(&candidate, Event::PolicyEvaluate, 0, &ask);
        assert_eq!(waiting.state, State::Candidate);
        assert!(effects.iter().any(|e| matches!(e, Effect::Notify { kind: NotifyKind::OfferStart, actions } if actions.contains(&Action::Start))));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::StartCapture | Effect::SetDeadline { .. })),
            "ask mode writes no audio byte until an explicit start"
        );
        // an explicit start from the toast starts capture
        let (recording, effects) = apply(
            &waiting,
            Event::CommandStart {
                client: "engine-notification".into(),
            },
            1_000,
            &ask,
        );
        assert_eq!(recording.state, State::Recording);
        assert!(effects.contains(&Effect::StartCapture));
        // ask with no surface of either kind: suppression, no capture
        let no_surface = inputs_no_surface(Mode::Ask);
        let candidate = start(&SessionState::idle(), 0, &no_surface);
        let (discarded, effects) = apply(&candidate, Event::PolicyEvaluate, 0, &no_surface);
        assert_eq!(discarded.state, State::Discarded);
        assert!(effects.contains(&Effect::RecordSuppression {
            cause: "no_consent_surface".into()
        }));
        assert!(!effects.contains(&Effect::StartCapture));
        // auto with no surface: same suppression; auto with only a deliverable notification: arms
        let no_surface = inputs_no_surface(Mode::Auto);
        let candidate = start(&SessionState::idle(), 0, &no_surface);
        let (discarded, effects) = apply(&candidate, Event::PolicyEvaluate, 0, &no_surface);
        assert_eq!(discarded.state, State::Discarded);
        assert!(effects.contains(&Effect::RecordSuppression {
            cause: "no_consent_surface".into()
        }));
        let toast_only = inputs(Mode::Auto, true, Some(0));
        let candidate = start(&SessionState::idle(), 0, &toast_only);
        let (armed, effects) = apply(&candidate, Event::PolicyEvaluate, 0, &toast_only);
        assert_eq!(armed.state, State::Arming);
        assert!(effects.iter().any(|e| matches!(e, Effect::Notify { kind: NotifyKind::Countdown { seconds: 10 }, actions } if actions.contains(&Action::Cancel))));
        // manual: no notification of any kind
        let manual = inputs(Mode::Manual, true, Some(0));
        let candidate = start(&SessionState::idle(), 0, &manual);
        let (_, effects) = apply(&candidate, Event::PolicyEvaluate, 0, &manual);
        assert!(!effects.iter().any(|e| matches!(e, Effect::Notify { .. })));
    }

    fn inputs_no_surface(mode: Mode) -> StepInputs {
        let mut i = inputs(mode, false, Some(0));
        i.consent.clients = vec![ClientCapabilities {
            client: "tray".into(),
            indicator: true,
            cancel: false,
        }];
        i
    }

    #[test]
    fn cancel_suppresses_rearm_for_identity() {
        let inputs_at = |t: u64| inputs(Mode::Auto, true, Some(t));
        let candidate = start(&SessionState::idle(), 0, &inputs_at(0));
        let (armed, _) = apply(&candidate, Event::PolicyEvaluate, 0, &inputs_at(0));
        assert_eq!(armed.state, State::Arming);
        let (cancelled, effects) = apply(
            &armed,
            Event::CommandCancel {
                client: "engine-notification".into(),
            },
            5_000,
            &inputs_at(5_000),
        );
        assert_eq!(cancelled.state, State::Discarded);
        assert!(effects.contains(&Effect::SuppressRearm {
            identity: identity(),
            quiet_ms: CANCEL_QUIET_MS
        }));
        assert!(
            effects.contains(&Effect::DiscardEmptyArtifacts),
            "a cancelled countdown leaves no chunk file"
        );
        // the machine returns to idle for the next decision but remembers the cancel
        let mut idle = cancelled.clone();
        idle.state = State::Idle;
        idle.session_id = None;
        // signals still present at t=100 s: the next tick must not re-arm
        let out = step(
            &idle,
            &Event::DetectorStart {
                identity: identity(),
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(101_000),
            &inputs_at(100_000),
        );
        assert_eq!(
            out.next_state(),
            State::Idle,
            "re-arming is suppressed while the identity keeps being seen"
        );
        assert!(matches!(out, StepOutcome::Accepted { .. }));
        // absent for 61 s: the next decision may arm again with a fresh countdown
        let out = step(
            &idle,
            &Event::DetectorStart {
                identity: identity(),
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(161_000),
            &inputs_at(100_000),
        );
        assert_eq!(out.next_state(), State::Candidate);
        // a different identity was never suppressed
        let other = MeetingIdentity {
            adapter_id: "adapter-b".into(),
            subject_key: "tab:9".into(),
        };
        let out = step(
            &idle,
            &Event::DetectorStart {
                identity: other,
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(101_000),
            &inputs_at(100_000),
        );
        assert_eq!(out.next_state(), State::Candidate);
    }

    #[test]
    fn suspend_during_countdown_reevaluates() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let candidate = start(&SessionState::idle(), 0, &inputs);
        let (armed, _) = apply(&candidate, Event::PolicyEvaluate, 0, &inputs);
        assert_eq!(
            armed.deadlines.at(DeadlineKind::Countdown),
            Some(Unbiased(COUNTDOWN_MS))
        );
        let (suspended, _) = apply(&armed, Event::SystemSuspend, 3_000, &inputs);
        assert_eq!(suspended.state, State::Arming);
        let resume_at = 3_000 + 30 * 60 * 1_000;
        let live = StepInputs {
            identity_last_seen: Some(Unbiased(resume_at)),
            ..inputs.clone()
        };
        let (resumed, effects) = apply(&suspended, Event::SystemResume, resume_at, &live);
        assert_eq!(
            resumed.state,
            State::Candidate,
            "resume re-evaluates instead of firing the countdown"
        );
        assert!(effects.contains(&Effect::EvaluatePolicy));
        assert!(!effects.contains(&Effect::StartCapture));
        assert!(resumed.deadlines.at(DeadlineKind::Countdown).is_none());
        let (rearmed, effects) = apply(&resumed, Event::PolicyEvaluate, resume_at + 1, &live);
        assert_eq!(rearmed.state, State::Arming);
        assert!(
            effects.contains(&Effect::SetDeadline {
                kind: DeadlineKind::Countdown,
                at: Unbiased(resume_at + 1 + COUNTDOWN_MS)
            }),
            "a new full countdown"
        );
        // if the meeting is gone by resume, no capture and an empty directory is discarded
        let gone = StepInputs {
            identity_last_seen: None,
            ..inputs.clone()
        };
        let (resumed, _) = apply(&suspended, Event::SystemResume, resume_at, &gone);
        let (discarded, effects) = apply(&resumed, Event::PolicyEvaluate, resume_at, &gone);
        assert_eq!(discarded.state, State::Discarded);
        assert!(effects.contains(&Effect::DiscardEmptyArtifacts));
        // a stale timer arriving after the resume is rejected, never applied
        assert!(matches!(
            step(&resumed, &Event::TimerCountdown, Unbiased(resume_at), &gone),
            StepOutcome::Rejected { .. }
        ));
    }

    #[test]
    fn flapping_end_signal_yields_one_session() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let session_id = SessionId::new();
        let mut machine = SessionState::restored(State::Recording, session_id);
        machine.identity = Some(identity());
        let mut finalize_effects = 0;
        let mut now = 0u64;
        for _ in 0..24 {
            let (ending, effects) = apply(
                &machine,
                Event::DetectorEnd {
                    decision: decision(),
                },
                now,
                &inputs,
            );
            assert_eq!(ending.state, State::Ending);
            assert!(effects.contains(&Effect::SetDeadline {
                kind: DeadlineKind::Hysteresis,
                at: Unbiased(now + 60_000)
            }));
            now += 5_000;
            let (recording, effects) = apply(
                &ending,
                Event::DetectorContinues {
                    decision: decision(),
                },
                now,
                &inputs,
            );
            assert_eq!(recording.state, State::Recording);
            assert!(
                !effects.iter().any(|e| matches!(e, Effect::StartCapture)),
                "the same tracks continue: no new files"
            );
            finalize_effects += effects
                .iter()
                .filter(|e| matches!(e, Effect::Finalize))
                .count();
            assert_eq!(recording.session_id, Some(session_id));
            machine = recording;
        }
        assert_eq!(finalize_effects, 0);
        // the genuine end: hysteresis expires, prompt, no answer, finalize exactly once
        let (ending, _) = apply(
            &machine,
            Event::DetectorEnd {
                decision: decision(),
            },
            now,
            &inputs,
        );
        let (prompting, effects) = apply(&ending, Event::TimerHysteresis, now + 60_000, &inputs);
        assert_eq!(prompting.state, State::Ending);
        assert!(effects.iter().any(|e| matches!(e, Effect::Notify { kind: NotifyKind::StillInMeeting, actions } if actions.contains(&Action::Yes) && actions.contains(&Action::No))));
        let (extended, effects) = apply(
            &prompting,
            Event::CommandExtendYes {
                client: "engine-notification".into(),
            },
            now + 70_000,
            &inputs,
        );
        assert_eq!(extended.state, State::Ending);
        assert!(effects.contains(&Effect::SetDeadline {
            kind: DeadlineKind::Extension,
            at: Unbiased(now + 70_000 + 300_000)
        }));
        assert!(extended.extension_used);
        let (finalizing, effects) = apply(&extended, Event::TimerExtension, now + 370_000, &inputs);
        assert_eq!(
            finalizing.state,
            State::Finalizing,
            "the second expiry finalizes without a further prompt"
        );
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::Finalize))
                .count(),
            1
        );
        assert_eq!(finalizing.session_id, Some(session_id));
    }

    fn accept(
        machine: &SessionState,
        event: Event,
        now: u64,
        inputs: &StepInputs,
    ) -> (SessionState, Vec<Effect>) {
        match step(machine, &event, Unbiased(now), inputs) {
            StepOutcome::Accepted { next, effects, .. } => (next, effects),
            StepOutcome::Rejected { state, event } => panic!("rejected {event:?} in {state:?}"),
        }
    }

    #[test]
    fn finished_session_does_not_block_the_next_meeting() {
        // completed / discarded / failed are memory, not walls: a new detector start opens a new session
        let inputs = inputs(Mode::Auto, true, Some(0));
        for terminal in [State::Completed, State::Discarded, State::Failed] {
            let mut done = SessionState::idle();
            done.state = terminal;
            done.session_id = Some(SessionId::new());
            let next = start(&done, 10, &inputs);
            assert_eq!(
                next.state,
                State::Candidate,
                "{terminal:?} → candidate on detector_start"
            );
            assert_ne!(next.session_id, done.session_id, "a fresh session id");
        }
        // interrupted is not terminal: recovery decides
        assert!(!State::Interrupted.is_terminal());
        assert!(
            State::Completed.is_terminal()
                && State::Discarded.is_terminal()
                && State::Failed.is_terminal()
        );
        assert!(!State::Recording.is_terminal() && !State::Ending.is_terminal());
    }

    #[test]
    fn cancel_quiet_period_survives_a_finished_session() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let mut discarded = SessionState::idle();
        discarded.state = State::Discarded;
        discarded.cancelled.insert(identity(), Unbiased(0));
        let out = step(
            &discarded,
            &Event::DetectorStart {
                identity: identity(),
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(1_000),
            &inputs,
        );
        assert_eq!(
            out.next_state(),
            State::Discarded,
            "still inside the quiet period"
        );
        assert!(out.effects().contains(&Effect::RecordSuppression {
            cause: "cancel_quiet_period".into()
        }));
    }

    #[test]
    fn extension_is_granted_once_per_ending_episode() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let mut recording = SessionState::idle();
        recording.state = State::Recording;
        recording.session_id = Some(SessionId::new());
        // first episode: end → prompt → extend yes → the meeting continues
        let (ending, _) = accept(
            &recording,
            Event::DetectorEnd {
                decision: decision(),
            },
            10,
            &inputs,
        );
        let (prompted, _) = accept(&ending, Event::TimerHysteresis, 60_010, &inputs);
        let (extended, _) = accept(
            &prompted,
            Event::CommandExtendYes {
                client: "ui".into(),
            },
            60_020,
            &inputs,
        );
        assert!(extended.extension_used);
        let (back, effects) = accept(
            &extended,
            Event::DetectorContinues {
                decision: decision(),
            },
            61_000,
            &inputs,
        );
        assert_eq!(back.state, State::Recording);
        assert!(effects.contains(&Effect::ResetExtension));
        assert!(
            !back.extension_used,
            "a new ending episode gets its own extension"
        );
        // second episode prompts again instead of finalizing silently
        let (ending2, _) = accept(
            &back,
            Event::DetectorEnd {
                decision: decision(),
            },
            200_000,
            &inputs,
        );
        let (after, effects) = accept(&ending2, Event::TimerHysteresis, 260_000, &inputs);
        assert_eq!(after.state, State::Ending);
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Notify {
                kind: NotifyKind::StillInMeeting,
                ..
            }
        )));
    }

    #[test]
    fn paused_session_fails_and_resumes_capture_honestly() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let mut paused = SessionState::idle();
        paused.state = State::Paused;
        paused.session_id = Some(SessionId::new());
        let (failed, effects) = accept(
            &paused,
            Event::CaptureFailed {
                reason: "device_lost".into(),
            },
            5,
            &inputs,
        );
        assert_eq!(failed.state, State::Failed);
        assert!(effects.contains(&Effect::RecordFailure {
            reason: "device_lost".into()
        }));
        let (still, effects) = accept(&paused, Event::ConsentLost, 5, &inputs);
        assert_eq!(still.state, State::Paused);
        assert!(effects.contains(&Effect::RecordIndicatorUnavailable));
        // an end signal while paused never resumes capture: the session stays paused, the hysteresis
        // window starts, and only command_resume brings audio back
        let (still_paused, effects) = accept(
            &paused,
            Event::DetectorEnd {
                decision: decision(),
            },
            5,
            &inputs,
        );
        assert_eq!(still_paused.state, State::Paused);
        assert!(
            !effects.contains(&Effect::ResumeCapture),
            "no capture without a command: {effects:?}"
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::SetDeadline {
                kind: DeadlineKind::Hysteresis,
                ..
            }
        )));
        let (back, effects) = accept(
            &still_paused,
            Event::DetectorContinues {
                decision: decision(),
            },
            6,
            &inputs,
        );
        assert_eq!(
            back.state,
            State::Paused,
            "a continuing signal does not restart a paused recording"
        );
        assert!(!effects.contains(&Effect::ResumeCapture));
        let (finalizing, effects) = accept(&still_paused, Event::TimerHysteresis, 60_006, &inputs);
        assert_eq!(
            finalizing.state,
            State::Finalizing,
            "the meeting ended while paused: finalize what was recorded"
        );
        assert!(effects.contains(&Effect::StopCapture) && effects.contains(&Effect::Finalize));
        let (resumed, effects) = accept(
            &still_paused,
            Event::CommandResume {
                client: "ui".into(),
            },
            7,
            &inputs,
        );
        assert_eq!(resumed.state, State::Recording);
        assert!(
            effects.contains(&Effect::ResumeCapture),
            "only the user's command resumes capture"
        );
        let mut ending_live = SessionState::idle();
        ending_live.state = State::Ending;
        let (failed, _) = accept(
            &ending_live,
            Event::CaptureFailed {
                reason: "device_lost".into(),
            },
            5,
            &inputs,
        );
        assert_eq!(failed.state, State::Failed);
    }

    #[test]
    fn unreadable_mode_store_is_surfaced_not_swallowed() {
        let mut inputs = inputs(Mode::Auto, true, Some(0));
        inputs.mode_settings.readable = false;
        let out = step(
            &SessionState::idle(),
            &Event::DetectorStart {
                identity: identity(),
                class: AppClass::Desktop,
                decision: decision(),
            },
            Unbiased(1),
            &inputs,
        );
        assert!(
            out.effects().iter().any(|e| matches!(
                e,
                Effect::RecordModeDegraded {
                    effective: Mode::Manual,
                    ..
                }
            )),
            "{:?}",
            out.effects()
        );
    }

    #[test]
    fn snapshot_serializes_after_a_cancel() {
        let inputs = inputs(Mode::Auto, true, Some(0));
        let candidate = start(&SessionState::idle(), 1, &inputs);
        let (arming, _) = accept(&candidate, Event::PolicyEvaluate, 2, &inputs);
        assert_eq!(arming.state, State::Arming);
        let (cancelled, _) = accept(
            &arming,
            Event::CommandCancel {
                client: "ui".into(),
            },
            3,
            &inputs,
        );
        assert!(!cancelled.cancelled.is_empty());
        let json = serde_json::to_value(&cancelled)
            .expect("a snapshot with a cancelled identity serializes");
        let back: SessionState = serde_json::from_value(json).unwrap();
        assert_eq!(back, cancelled);
    }
}
