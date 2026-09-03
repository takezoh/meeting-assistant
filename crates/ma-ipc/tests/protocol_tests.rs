use ma_ipc::*;
use std::path::{Path, PathBuf};

const SID: &str = "S-1-5-21-1-2-3-1001";

/// A minimal authority: idle until `session.start`, then recording; `session.stop` ends it.
struct FakeAuthority {
    state: &'static str,
}
impl FakeAuthority {
    fn new() -> Self {
        FakeAuthority { state: "idle" }
    }
}
impl SessionAuthority for FakeAuthority {
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({ "state": self.state, "session_id": null })
    }
    fn apply(&mut self, method: &Method, _now_ms: u64) -> Result<Vec<Transition>, RpcError> {
        let cause = TransitionCause {
            kind: "command".into(),
            refs: vec!["ipc".into()],
        };
        let (from, to) = match (method, self.state) {
            (Method::SessionStart, "idle") => ("idle", "recording"),
            (Method::SessionStop, "recording") => ("recording", "ending"),
            (Method::ModeSet { .. }, _) => return Ok(vec![]),
            _ => {
                return Err(RpcError::new(
                    ErrorCode::Rejected,
                    "not applicable in this state",
                ))
            }
        };
        self.state = to;
        Ok(vec![Transition {
            from: from.into(),
            to: to.into(),
            cause,
        }])
    }
}

fn hello_frame(id: u64, version: &str) -> Frame {
    Frame::request(
        id,
        "engine.hello",
        Some(
            serde_json::json!({ "client_protocol": version, "client_capabilities": ["indicator", "cancel"] }),
        ),
    )
}

fn handshake(
    dispatcher: &mut Dispatcher<FakeAuthority>,
    conn: &mut Connection<DuplexEnd>,
    client: &mut DuplexEnd,
) -> Frame {
    client
        .send(&hello_frame(1, &PROTOCOL_VERSION.to_string()))
        .unwrap();
    dispatcher.pump(conn, 0);
    client.recv().unwrap().expect("hello response")
}

fn transition(from: &str, to: &str) -> EventBody {
    EventBody::SessionTransition {
        from: from.into(),
        to: to.into(),
        cause: TransitionCause {
            kind: "command".into(),
            refs: vec![],
        },
    }
}

fn fixtures() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/ipc/fixtures");
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out.into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn schema_golden_roundtrip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/ipc");
    let protocol: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("protocol.schema.json")).unwrap()).unwrap();
    let methods: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("methods.schema.json")).unwrap()).unwrap();
    let protocol_validator =
        jsonschema::validator_for(&protocol).expect("protocol schema compiles");
    let methods_validator = jsonschema::validator_for(&methods).expect("methods schema compiles");
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= METHOD_NAMES.len() + event::EVENT_NAMES.len(),
        "one fixture per method and per event at least: {}",
        fixtures.len()
    );
    let mut seen_methods = std::collections::BTreeSet::new();
    let mut seen_events = std::collections::BTreeSet::new();
    for (name, value) in &fixtures {
        let frame: Frame =
            serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = serde_json::to_value(&frame).unwrap();
        assert_eq!(&back, value, "{name} round-trips byte-for-byte as JSON");
        let errors: Vec<String> = protocol_validator
            .iter_errors(value)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{name} violates protocol.schema.json: {errors:?}"
        );
        match &frame {
            Frame::Request { method, params, .. } => {
                let m = Method::parse(method, params.as_ref())
                    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                let envelope = serde_json::json!({ "method": method, "params": params });
                let errors: Vec<String> = methods_validator
                    .iter_errors(&envelope)
                    .map(|e| e.to_string())
                    .collect();
                assert!(
                    errors.is_empty(),
                    "{name} violates methods.schema.json: {errors:?}"
                );
                seen_methods.insert(m.name());
            }
            Frame::Notification { method, params, .. } => {
                let event: Event = serde_json::from_value(params.clone()).unwrap();
                assert_eq!(event.body.name(), method);
                seen_events.insert(event.body.name());
            }
            Frame::Response { result, .. } => {
                if let Some(snapshot) = result.get("session_snapshot") {
                    let snapshot_schema = serde_json::json!({ "$defs": methods["$defs"].clone(), "$ref": "#/$defs/session_snapshot" });
                    let v = jsonschema::validator_for(&snapshot_schema).unwrap();
                    let errors: Vec<String> =
                        v.iter_errors(snapshot).map(|e| e.to_string()).collect();
                    assert!(
                        errors.is_empty(),
                        "{name}: snapshot state is not a declared state: {errors:?}"
                    );
                }
            }
            _ => {}
        }
    }
    let mut expected_methods = METHOD_NAMES.to_vec();
    expected_methods.sort();
    assert_eq!(
        seen_methods.into_iter().collect::<Vec<_>>(),
        expected_methods
    );
    let mut expected_events = event::EVENT_NAMES.to_vec();
    expected_events.sort();
    assert_eq!(seen_events.into_iter().collect::<Vec<_>>(), expected_events);
    let declared: Vec<&str> = methods["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["properties"]["method"]["const"].as_str().unwrap())
        .collect();
    assert_eq!(
        declared,
        METHOD_NAMES.to_vec(),
        "the schema's method set is the code's method set"
    );
}

#[test]
fn handshake_major_mismatch_refused() {
    let mut d = Dispatcher::new(SID, FakeAuthority::new());
    let (server, mut client) = DuplexPair::pair();
    let mut conn = Connection::accept(server, Some(SID));
    client.send(&hello_frame(1, "2.0.0")).unwrap();
    d.pump(&mut conn, 0);
    let Frame::Error { error, id, .. } = client.recv().unwrap().unwrap() else {
        panic!("typed error expected")
    };
    assert_eq!(id, Some(1));
    assert_eq!(error.kind(), Some(ErrorCode::ProtocolMismatch));
    assert_eq!(
        error.data.as_ref().unwrap()["required_protocol"],
        PROTOCOL_VERSION.to_string()
    );
    assert_eq!(
        conn.state(),
        ConnectionState::Handshaking,
        "no partial operation on mismatch"
    );
    client
        .send(&Frame::request(2, "session.start", None))
        .unwrap();
    d.pump(&mut conn, 0);
    let Frame::Error { error, .. } = client.recv().unwrap().unwrap() else {
        panic!()
    };
    assert_eq!(error.kind(), Some(ErrorCode::NotHandshaken));
    assert_eq!(d.authority.state, "idle", "nothing happened");
    // a compatible minor bump is fine
    client.send(&hello_frame(3, "1.7.0")).unwrap();
    d.pump(&mut conn, 0);
    assert!(matches!(
        client.recv().unwrap().unwrap(),
        Frame::Response { id: 3, .. }
    ));
    assert_eq!(conn.state(), ConnectionState::Open);
}

#[test]
fn stalled_client_resyncs() {
    // A client that stops reading while transitions pile up is either cut off with ClientTooSlow
    // or sees a seq gap on resume; in both cases it renders nothing until it re-snapshots.
    let mut d = Dispatcher::new(SID, FakeAuthority::new());
    let (server, mut client_end) = DuplexPair::with_window(4);
    let mut conn = Connection::accept(server, Some(SID));
    let mut client = Client::default();
    let reply = handshake(&mut d, &mut conn, &mut client_end);
    client.observe(&reply);
    assert_eq!(
        client.rendered.as_ref().and_then(|s| s["state"].as_str()),
        Some("idle")
    );
    // the UI stalls; the engine keeps publishing
    for _ in 0..(GENERAL_QUEUE + 40) {
        conn.publish(EventBody::CaptureLevel {
            track: "mic".into(),
            rms: -20,
        });
    }
    assert!(
        conn.dropped_levels > 0,
        "levels are dropped oldest-first, never blocking"
    );
    assert_eq!(conn.state(), ConnectionState::Open);
    for _ in 0..TRANSITION_RESERVE {
        conn.publish(transition("idle", "recording"));
    }
    conn.flush();
    assert_eq!(
        conn.state(),
        ConnectionState::Open,
        "the reserve holds exactly the reserved count"
    );
    conn.publish(transition("recording", "ending"));
    assert_eq!(
        conn.state(),
        ConnectionState::Closed(CloseReason::ClientTooSlow),
        "transitions are never dropped; the client is"
    );
    // the UI resumes and reads what it can
    let mut saw_disconnect = false;
    let mut stale_render = false;
    while let Ok(Some(frame)) = client_end.recv() {
        match client.observe(&frame) {
            ClientAction::Disconnected(CloseReason::ClientTooSlow) => saw_disconnect = true,
            ClientAction::Rendered { .. } if client.needs_snapshot => stale_render = true,
            _ => {}
        }
    }
    assert!(saw_disconnect || client.needs_snapshot);
    assert!(!stale_render, "never renders from stale events");
    assert!(
        client.rendered.is_none(),
        "local state discarded until the next snapshot"
    );

    // Second path: a gap without a disconnect (a slow but not wedged client) forces a re-snapshot.
    let mut d = Dispatcher::new(SID, FakeAuthority::new());
    let (server, mut client_end) = DuplexPair::with_window(64);
    let mut conn = Connection::accept(server, Some(SID));
    let mut client = Client::default();
    let reply = handshake(&mut d, &mut conn, &mut client_end);
    client.observe(&reply);
    for _ in 0..(GENERAL_QUEUE + 10) {
        conn.publish(EventBody::CaptureLevel {
            track: "mic".into(),
            rms: -20,
        });
    }
    conn.flush();
    let mut gap = None;
    while let Ok(Some(frame)) = client_end.recv() {
        if let ClientAction::ResnapshotRequired { expected, got } = client.observe(&frame) {
            gap = Some((expected, got));
            break;
        }
    }
    assert!(
        gap.is_some(),
        "the dropped levels leave a seq gap the client must notice"
    );
    assert!(client.rendered.is_none());
    client_end
        .send(&Frame::request(9, "session.snapshot", None))
        .unwrap();
    d.pump(&mut conn, 1);
    // whatever is still in flight is suppressed; only the snapshot response restores rendering
    loop {
        let frame = client_end
            .recv()
            .unwrap()
            .expect("the snapshot response arrives");
        match client.observe(&frame) {
            ClientAction::Response(9) => break,
            ClientAction::Rendered { .. } => panic!("rendered before the snapshot"),
            _ => {}
        }
    }
    assert_eq!(
        client.rendered.as_ref().and_then(|s| s["state"].as_str()),
        Some("idle")
    );
    assert!(!client.needs_snapshot);
}

#[test]
fn authz_foreign_sid_rejected_before_dispatch() {
    let mut d = Dispatcher::new(SID, FakeAuthority::new());
    for foreign in [Some("S-1-5-21-9-9-9-1002"), None] {
        let (server, mut client) = DuplexPair::pair();
        let mut conn = Connection::accept(server, foreign);
        client
            .send(&hello_frame(1, &PROTOCOL_VERSION.to_string()))
            .unwrap();
        d.pump(&mut conn, 0);
        assert_eq!(
            conn.state(),
            ConnectionState::Closed(CloseReason::Unauthorized),
            "{foreign:?}"
        );
        assert_eq!(conn.dispatched, 0, "dispatch is unreachable on mismatch");
        assert!(
            client.recv().is_err() || client.recv().unwrap().is_none(),
            "no snapshot, no event, no response leaks"
        );
    }
    assert_eq!(authorize_client(SID, Some(SID)), Ok(()));
    assert_eq!(
        authorize_client(SID, Some("S-1-5-21-9-9-9-1002")),
        Err(AuthzError::SidMismatch {
            sid: "S-1-5-21-9-9-9-1002".into()
        })
    );
    assert_eq!(
        authorize_client(SID, None),
        Err(AuthzError::ImpersonationFailed)
    );
}

#[test]
fn authz_build_channel_carveout() {
    let ctx = ClientContext {
        installed_engine_path: PathBuf::from(r"C:\Program Files\MeetingAssistant\ma-engine.exe"),
        own_target_dir: PathBuf::from(r"C:\src\meeting-assistant\target"),
    };
    let unsigned_dev = ServerImage {
        image_path: PathBuf::from(r"C:\src\meeting-assistant\target\debug\ma-engine.exe"),
        same_user_sid: true,
        signature: SignatureStatus::Unsigned,
    };
    let unsigned_elsewhere = ServerImage {
        image_path: PathBuf::from(r"C:\Users\x\Downloads\ma-engine.exe"),
        same_user_sid: true,
        signature: SignatureStatus::Unsigned,
    };
    let other_user_dev = ServerImage {
        same_user_sid: false,
        ..unsigned_dev.clone()
    };
    let installed_signed = ServerImage {
        image_path: ctx.installed_engine_path.clone(),
        same_user_sid: true,
        signature: SignatureStatus::ValidPinnedSigner,
    };
    let installed_unsigned = ServerImage {
        signature: SignatureStatus::Unsigned,
        ..installed_signed.clone()
    };
    let installed_unverifiable = ServerImage {
        signature: SignatureStatus::Unverifiable,
        ..installed_signed.clone()
    };
    // release: only the signed installed engine
    assert!(verify_server(BuildChannel::Release, &installed_signed, &ctx).is_ok());
    assert!(
        verify_server(BuildChannel::Release, &unsigned_dev, &ctx).is_err(),
        "release refuses an unsigned same-user server at a non-installed path"
    );
    assert!(verify_server(BuildChannel::Release, &installed_unsigned, &ctx).is_err());
    assert!(
        verify_server(BuildChannel::Release, &installed_unverifiable, &ctx).is_err(),
        "unverifiable is a mismatch, not a pass"
    );
    // development: own build tree, same user; an installed path still needs a signature
    assert!(verify_server(BuildChannel::Development, &unsigned_dev, &ctx).is_ok());
    assert!(verify_server(BuildChannel::Development, &unsigned_elsewhere, &ctx).is_err());
    assert!(verify_server(BuildChannel::Development, &other_user_dev, &ctx).is_err());
    assert!(verify_server(BuildChannel::Development, &installed_unsigned, &ctx).is_err());
    assert!(verify_server(BuildChannel::Development, &installed_signed, &ctx).is_ok());
    // no runtime input flips the channel
    let compiled = BuildChannel::compiled();
    std::env::set_var("MA_BUILD_CHANNEL", "development");
    std::env::set_var("MA_DEV", "1");
    assert_eq!(BuildChannel::compiled(), compiled);
    assert_eq!(
        compiled,
        if cfg!(feature = "development") {
            BuildChannel::Development
        } else {
            BuildChannel::Release
        }
    );
}

#[test]
fn unknown_method_is_typed_and_malformed_frame_closes_with_byte_count_only() {
    let mut d = Dispatcher::new(SID, FakeAuthority::new());
    let (server, mut client) = DuplexPair::pair();
    let mut conn = Connection::accept(server, Some(SID));
    handshake(&mut d, &mut conn, &mut client);
    client
        .send(&Frame::request(5, "session.teleport", None))
        .unwrap();
    d.pump(&mut conn, 0);
    let Frame::Error { error, .. } = client.recv().unwrap().unwrap() else {
        panic!()
    };
    assert_eq!(error.kind(), Some(ErrorCode::MethodNotFound));
    let payload =
        b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"session.start\",\"params\":\"ZZ-SECRET-ZZ";
    let (server, _client) = DuplexPair::pair();
    let mut conn = Connection::accept(server, Some(SID));
    conn.transport_mut().inject_raw(payload.to_vec());
    d.pump(&mut conn, 0);
    assert_eq!(
        conn.state(),
        ConnectionState::Closed(CloseReason::MalformedFrame {
            bytes: payload.len()
        })
    );
    assert!(!format!("{:?}", conn.state()).contains("ZZ-SECRET"));
}

/// Regenerate `contracts/ipc/fixtures/` from the Rust types:
/// `cargo test -p ma-ipc --test protocol_tests write_fixtures -- --ignored`.
/// The fixtures are then validated against the hand-written schemas by `schema_golden_roundtrip`.
#[test]
#[ignore]
fn write_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/ipc/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    for stale in std::fs::read_dir(&dir).unwrap().flatten() {
        std::fs::remove_file(stale.path()).unwrap();
    }
    let artifact = ma_core_types::ArtifactId::from_uuid(uuid_const(0x11));
    let meeting = ma_core_types::MeetingId::from_uuid(uuid_const(0x22));
    let methods = vec![
        Method::EngineHello(Hello {
            client_protocol: PROTOCOL_VERSION,
            client_capabilities: vec![
                Capability::Indicator,
                Capability::Cancel,
                Capability::Notify,
            ],
        }),
        Method::SessionSnapshot,
        Method::SessionStart,
        Method::SessionStop,
        Method::SessionPause,
        Method::SessionResume,
        Method::SessionDiscard,
        Method::SessionCancelArming,
        Method::SessionExtendHysteresis { extend: true },
        Method::ModeSet {
            scope: ModeScope::Class {
                class: AppClassName::Browser,
            },
            mode: ModeName::Ask,
        },
        Method::ArtifactEdit {
            artifact_id: artifact,
            edit_revision: 3,
        },
        Method::MeetingDelete {
            meeting_id: meeting,
        },
        Method::DiagnosticsExport,
        Method::EngineShutdown,
    ];
    let mut files: Vec<(String, Frame)> = Vec::new();
    for (i, m) in methods.iter().enumerate() {
        files.push((
            format!("request-{:02}-{}.json", i + 1, m.name().replace('.', "-")),
            Frame::request(i as u64 + 1, m.name(), m.params()),
        ));
    }
    let hello = HelloResult {
        engine_protocol: PROTOCOL_VERSION,
        engine_version: "0.1.0".into(),
        session_snapshot: serde_json::json!({ "state": "idle", "session_id": null }),
        event_seq: 0,
    };
    files.push((
        "response-engine-hello.json".into(),
        Frame::response(1, serde_json::to_value(&hello).unwrap()),
    ));
    files.push(("response-session-snapshot.json".into(), Frame::response(2, serde_json::json!({ "session_snapshot": { "state": "recording", "session_id": "01990ce0-1000-7000-8000-000000000042" }, "event_seq": 17 }))));
    files.push((
        "error-protocol-mismatch.json".into(),
        Frame::error(
            Some(1),
            RpcError::protocol_mismatch(
                PROTOCOL_VERSION,
                ProtocolVersion {
                    major: 2,
                    minor: 0,
                    patch: 0,
                },
            ),
        ),
    ));
    files.push((
        "error-method-not-found.json".into(),
        Frame::error(Some(5), RpcError::method_not_found("session.teleport")),
    ));
    files.push((
        "error-client-too-slow.json".into(),
        Frame::error(
            None,
            RpcError::new(ErrorCode::ClientTooSlow, "client too slow"),
        ),
    ));
    let events = vec![
        EventBody::SessionTransition {
            from: "arming".into(),
            to: "recording".into(),
            cause: TransitionCause {
                kind: "timer".into(),
                refs: vec!["countdown".into()],
            },
        },
        EventBody::CaptureLevel {
            track: "mic".into(),
            rms: -23,
        },
        EventBody::CaptureDegraded {
            reason: "device_lost".into(),
        },
        EventBody::ArmingTick { remaining_ms: 7000 },
        EventBody::DetectorDecision {
            outcome: "determinate_start".into(),
            evidence: vec!["01990cdf-881f-7000-b000-1328b7bc6f00".into()],
        },
        EventBody::Error {
            code: -32005,
            message: "not applicable in this state".into(),
        },
    ];
    for (i, body) in events.into_iter().enumerate() {
        let name = body.name().replace('.', "-");
        let event = Event {
            seq: 100 + i as u64,
            body,
        };
        files.push((
            format!("event-{:02}-{name}.json", i + 1),
            Frame::notification(event.body.name(), serde_json::to_value(&event).unwrap()),
        ));
    }
    for (name, frame) in files {
        std::fs::write(
            dir.join(name),
            serde_json::to_string_pretty(&frame).unwrap() + "\n",
        )
        .unwrap();
    }
}

fn uuid_const(fill: u8) -> uuid::Uuid {
    let mut bytes = [fill; 16];
    bytes[6] = 0x70 | (bytes[6] & 0x0f);
    bytes[8] = 0x80 | (bytes[8] & 0x3f);
    uuid::Uuid::from_bytes(bytes)
}
