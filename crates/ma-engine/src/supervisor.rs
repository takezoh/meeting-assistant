//! Instance lock, update deferral and the processor-host launch seam.

use ma_secure::acl::PipeSecurity;
use ma_session::State;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockError {
    /// The pipe name already exists: another engine is running. Exit without side effects.
    EngineAlreadyRunning,
    Unsupported,
}

/// The single-instance lock is the successful creation of the control pipe.
pub trait InstanceLock {
    type Guard;
    fn acquire(&mut self, pipe: &PipeSecurity) -> Result<Self::Guard, LockError>;
}

/// The engine control pipe with its owner-only descriptor and first-instance flag.
pub fn engine_pipe(installation_id: &str, owner_sid: &str) -> PipeSecurity {
    PipeSecurity::engine_pipe(installation_id, owner_sid)
}

/// Phase 0 has no Windows implementation on this tier; the platform unit provides it.
pub fn platform_lock(installation_id: &str) -> Result<(), LockError> {
    let _ = engine_pipe(installation_id, "S-1-0-0");
    Err(LockError::Unsupported)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOffer {
    pub version: String,
    pub staged_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDisposition {
    /// The running binary stays in place; the offer is kept and re-evaluated at the next terminal transition.
    Deferred {
        until_session_terminal: bool,
    },
    Apply,
}

/// The engine-side supervisor: session-aware update gate and processor host launches.
#[derive(Debug)]
pub struct Supervisor {
    pub session_state: State,
    pub pending_update: Option<UpdateOffer>,
    pub applied: Vec<UpdateOffer>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Supervisor {
            session_state: State::Idle,
            pending_update: None,
            applied: Vec::new(),
        }
    }
}

impl Supervisor {
    /// An update is applied only when no session is non-terminal.
    pub fn offer_update(&mut self, offer: UpdateOffer) -> UpdateDisposition {
        if session_is_non_terminal(self.session_state) {
            self.pending_update = Some(offer);
            UpdateDisposition::Deferred {
                until_session_terminal: true,
            }
        } else {
            self.applied.push(offer);
            UpdateDisposition::Apply
        }
    }

    /// Called on every session transition; a pending offer applies once the session is terminal.
    pub fn on_transition(&mut self, to: State) -> Option<UpdateDisposition> {
        self.session_state = to;
        if !session_is_non_terminal(to) {
            if let Some(offer) = self.pending_update.take() {
                self.applied.push(offer);
                return Some(UpdateDisposition::Apply);
            }
        }
        None
    }
}

fn session_is_non_terminal(state: State) -> bool {
    // `idle` has no session; everything else that is not terminal (including `interrupted`, which
    // proceeds to finalizing on recovery) keeps the binary in place
    state != State::Idle && !state.is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> UpdateOffer {
        UpdateOffer {
            version: "0.2.0".into(),
            staged_path: PathBuf::from("staged/ma-engine.exe"),
        }
    }

    #[test]
    fn engine_pipe_is_owner_only_and_first_instance() {
        let pipe = engine_pipe("inst-01", "S-1-5-21-1-2-3-1001");
        assert_eq!(pipe.name, r"\\.\pipe\MeetingAssistant.engine.inst-01");
        assert!(pipe.first_pipe_instance);
        assert!(pipe.descriptor.grants_owner_only());
    }

    #[test]
    fn update_deferred_while_session_non_terminal() {
        let mut s = Supervisor::default();
        for state in [
            State::Candidate,
            State::Arming,
            State::Recording,
            State::Paused,
            State::Ending,
            State::Finalizing,
            State::Interrupted,
        ] {
            s.session_state = state;
            assert_eq!(
                s.offer_update(offer()),
                UpdateDisposition::Deferred {
                    until_session_terminal: true
                },
                "{state:?}"
            );
            assert!(
                s.applied.is_empty(),
                "the running binary stays in place during {state:?}"
            );
        }
        assert_eq!(s.on_transition(State::Ending), None, "still non-terminal");
        assert_eq!(
            s.on_transition(State::Completed),
            Some(UpdateDisposition::Apply)
        );
        assert_eq!(s.applied.len(), 1);
        let mut idle = Supervisor::default();
        assert_eq!(idle.offer_update(offer()), UpdateDisposition::Apply);
    }
}

/// `ma-session` behind the wire seam: every control method is applied through `step`, so its
/// effect is exactly what the next snapshot shows.
pub mod authority {
    use ma_ipc::{
        AppClassName, Method, ModeName, ModeScope, RpcError, SessionAuthority, Transition,
        TransitionCause,
    };
    use ma_session::state::step;
    use ma_session::{AppClass, Event, Mode, SessionState, StepInputs, StepOutcome, Unbiased};

    pub struct EngineAuthority {
        pub session: SessionState,
        pub inputs: StepInputs,
    }

    fn state_name(state: ma_session::State) -> String {
        serde_json::to_value(state)
            .expect("state serializes")
            .as_str()
            .expect("state is a string")
            .to_string()
    }

    fn mode(m: ModeName) -> Mode {
        match m {
            ModeName::Auto => Mode::Auto,
            ModeName::Ask => Mode::Ask,
            ModeName::Manual => Mode::Manual,
        }
    }

    impl SessionAuthority for EngineAuthority {
        fn snapshot(&self) -> serde_json::Value {
            serde_json::to_value(&self.session).expect("snapshot serializes")
        }

        fn apply(&mut self, method: &Method, now_ms: u64) -> Result<Vec<Transition>, RpcError> {
            let client = "ipc".to_string();
            let event = match method {
                Method::SessionStart => Event::CommandStart { client },
                Method::SessionStop => Event::CommandStop { client },
                Method::SessionPause => Event::CommandPause { client },
                Method::SessionResume => Event::CommandResume { client },
                Method::SessionDiscard => Event::CommandDiscard { client },
                Method::SessionCancelArming => Event::CommandCancel { client },
                Method::SessionExtendHysteresis { extend: true } => {
                    Event::CommandExtendYes { client }
                }
                Method::SessionExtendHysteresis { extend: false } => {
                    Event::CommandExtendNo { client }
                }
                Method::ModeSet { scope, mode: m } => {
                    match scope {
                        ModeScope::Global => self.inputs.mode_settings.global = mode(*m),
                        ModeScope::Class { class } => {
                            let class = match class {
                                AppClassName::Desktop => AppClass::Desktop,
                                AppClassName::Browser => AppClass::Browser,
                            };
                            self.inputs
                                .mode_settings
                                .class_defaults
                                .insert(class, mode(*m));
                        }
                        ModeScope::Adapter { adapter_id } => {
                            self.inputs
                                .mode_settings
                                .overrides
                                .insert(adapter_id.clone(), mode(*m));
                        }
                    }
                    return Ok(Vec::new());
                }
                // Owned by later units (workflow, store, diagnostics, supervisor); the method set is
                // closed in ma-ipc, and a method this build does not perform is a typed error.
                Method::ArtifactEdit { .. }
                | Method::MeetingDelete { .. }
                | Method::DiagnosticsExport
                | Method::EngineShutdown => {
                    return Err(RpcError::new(
                        ma_ipc::ErrorCode::NotImplemented,
                        "not implemented in this build",
                    )
                    .with_data(serde_json::json!({ "method": method.name() })));
                }
                Method::EngineHello(_) | Method::SessionSnapshot => {
                    unreachable!("handled by the dispatcher")
                }
            };
            match step(&self.session, &event, Unbiased(now_ms), &self.inputs) {
                StepOutcome::Accepted { next, record, .. } => {
                    let transition = Transition {
                        from: state_name(record.from),
                        to: state_name(record.to),
                        cause: TransitionCause {
                            kind: serde_json::to_value(record.cause.kind)
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_string(),
                            refs: record.cause.refs.clone(),
                        },
                    };
                    self.session = next;
                    Ok(vec![transition])
                }
                StepOutcome::Rejected { state, event } => Err(RpcError::new(
                    ma_ipc::ErrorCode::Rejected,
                    "not applicable in this state",
                )
                .with_data(serde_json::json!({ "state": state, "event": format!("{event:?}") }))),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ma_core_types::id::TypedId;
        use ma_ipc::{Connection, DuplexPair, Frame, Transport, PROTOCOL_VERSION};
        use ma_session::{ConsentSurfaces, MeetingIdentity, ModeSettings, State};

        fn authority() -> EngineAuthority {
            EngineAuthority {
                session: SessionState::default(),
                inputs: StepInputs {
                    mode_settings: ModeSettings {
                        global: Mode::Manual,
                        class_defaults: Default::default(),
                        overrides: Default::default(),
                        readable: true,
                    },
                    consent: ConsentSurfaces {
                        notification_deliverable: true,
                        clients: vec![],
                    },
                    identity_last_seen: None,
                    next_session_id: ma_core_types::SessionId::new(),
                },
            }
        }

        fn response(client: &mut DuplexPairEnd) -> serde_json::Value {
            let frame = client.recv().unwrap().unwrap();
            let Frame::Response { result, .. } = frame else {
                panic!("{frame:?}")
            };
            result
        }
        type DuplexPairEnd = ma_ipc::DuplexEnd;

        #[test]
        fn session_start_over_ipc_is_visible_in_the_next_snapshot() {
            const SID: &str = "S-1-5-21-1-2-3-1001";
            let mut d = ma_ipc::Dispatcher::new(SID, authority());
            let (server, mut client) = DuplexPair::pair();
            let mut conn = Connection::accept(server, Some(SID));
            client.send(&Frame::request(1, "engine.hello", Some(serde_json::json!({ "client_protocol": PROTOCOL_VERSION.to_string(), "client_capabilities": [] })))).unwrap();
            d.pump(&mut conn, 0);
            assert_eq!(response(&mut client)["session_snapshot"]["state"], "idle");
            // the detector raises a candidate (not an IPC method); the user then starts it over IPC
            let detected = Event::DetectorStart {
                identity: MeetingIdentity {
                    adapter_id: "desk-a".into(),
                    subject_key: "process:1:x".into(),
                },
                class: AppClass::Desktop,
                decision: ma_core_types::DecisionId::new(),
            };
            let StepOutcome::Accepted { next, .. } = step(
                &d.authority.session,
                &detected,
                Unbiased(5),
                &d.authority.inputs,
            ) else {
                panic!("candidate")
            };
            d.authority.session = next;
            d.authority.inputs.identity_last_seen = Some(Unbiased(5));
            assert_eq!(d.authority.session.state, State::Candidate);
            client
                .send(&Frame::request(2, "session.start", None))
                .unwrap();
            d.pump(&mut conn, 10);
            conn.flush();
            let result = response(&mut client);
            assert_eq!(d.authority.session.state, State::Recording);
            assert_eq!(result["session_snapshot"]["state"], "recording");
            let Frame::Notification { method, params, .. } = client.recv().unwrap().unwrap() else {
                panic!()
            };
            assert_eq!(method, "session.transition");
            assert_eq!(params["seq"], 1);
            assert_eq!(params["from"], "candidate");
            assert_eq!(params["to"], "recording");
        }

        #[test]
        fn snapshot_serializes_after_a_cancel_and_unimplemented_methods_are_typed_errors() {
            const SID: &str = "S-1-5-21-1-2-3-1001";
            let mut d = ma_ipc::Dispatcher::new(SID, authority());
            let (server, mut client) = DuplexPair::pair();
            let mut conn = Connection::accept(server, Some(SID));
            client.send(&Frame::request(1, "engine.hello", Some(serde_json::json!({ "client_protocol": PROTOCOL_VERSION.to_string(), "client_capabilities": ["indicator", "cancel"] })))).unwrap();
            d.pump(&mut conn, 0);
            let _ = response(&mut client);
            // detector start → policy → arming, then the user cancels: `cancelled` gains a struct key
            d.authority.inputs.mode_settings.global = Mode::Auto;
            d.authority.inputs.identity_last_seen = Some(Unbiased(5));
            let detected = Event::DetectorStart {
                identity: MeetingIdentity {
                    adapter_id: "desk-a".into(),
                    subject_key: "process:1:x".into(),
                },
                class: AppClass::Desktop,
                decision: ma_core_types::DecisionId::new(),
            };
            let StepOutcome::Accepted { next, .. } = step(
                &d.authority.session,
                &detected,
                Unbiased(5),
                &d.authority.inputs,
            ) else {
                panic!()
            };
            d.authority.session = next;
            let StepOutcome::Accepted { next, .. } = step(
                &d.authority.session,
                &Event::PolicyEvaluate,
                Unbiased(6),
                &d.authority.inputs,
            ) else {
                panic!()
            };
            d.authority.session = next;
            assert_eq!(d.authority.session.state, State::Arming);
            client
                .send(&Frame::request(2, "session.cancel_arming", None))
                .unwrap();
            d.pump(&mut conn, 7);
            conn.flush();
            let result = response(&mut client);
            assert_eq!(
                result["session_snapshot"]["state"], "discarded",
                "the snapshot after a cancel serializes and is served"
            );
            assert!(!d.authority.session.cancelled.is_empty());
            let _transition = client.recv().unwrap().unwrap();
            client
                .send(&Frame::request(3, "session.snapshot", None))
                .unwrap();
            d.pump(&mut conn, 8);
            assert_eq!(
                response(&mut client)["session_snapshot"]["state"],
                "discarded"
            );
            // methods this build does not perform answer with a typed error, not a silent success
            for method in ["meeting.delete", "engine.shutdown", "diagnostics.export"] {
                let params = if method == "meeting.delete" {
                    Some(
                        serde_json::json!({ "meeting_id": ma_core_types::MeetingId::new().to_string() }),
                    )
                } else {
                    None
                };
                client.send(&Frame::request(9, method, params)).unwrap();
                d.pump(&mut conn, 9);
                let frame = client.recv().unwrap().unwrap();
                let Frame::Error { error, .. } = frame else {
                    panic!("{method}: {frame:?}")
                };
                assert_eq!(
                    error.kind(),
                    Some(ma_ipc::ErrorCode::NotImplemented),
                    "{method}"
                );
            }
        }

        #[test]
        fn schema_state_names_are_the_session_states() {
            let schema: serde_json::Value =
                serde_json::from_str(include_str!("../../../contracts/ipc/methods.schema.json"))
                    .unwrap();
            let declared: Vec<String> = schema["$defs"]["state"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            let actual: Vec<String> = [
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
            ]
            .into_iter()
            .map(state_name)
            .collect();
            assert_eq!(declared, actual);
        }
    }
}
