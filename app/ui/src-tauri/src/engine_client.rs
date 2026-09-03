//! The engine client (contract-ipc-protocol, contract-consent-surface-precondition): connects over
//! an injected transport, declares `indicator` and `cancel` at the handshake, renders only the
//! engine's snapshot and events, re-snapshots after any disconnect or sequence gap, drives the
//! countdown from `arming.tick` events rather than a local timer, and reconnects with exponential
//! backoff. Headless: nothing here knows about WebView2.

use ma_ipc::{
    Capability, Client, ClientAction, CloseReason, Frame, Hello, Transport, TransportError,
    PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};

pub const CLIENT_CAPABILITIES: [Capability; 2] = [Capability::Indicator, Capability::Cancel];
pub const BACKOFF_MS: [u64; 6] = [500, 1_000, 2_000, 4_000, 8_000, 16_000];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Disconnected {
        next_attempt_ms: u64,
        attempts: u32,
    },
    Handshaking,
    Connected,
    /// A gap or a disconnect notice: nothing is rendered until the snapshot arrives.
    Resyncing,
}

/// Everything the frontend may render. Every field comes from the engine.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ViewState {
    pub connected: bool,
    pub session_state: Option<String>,
    pub session_id: Option<String>,
    /// From the latest `arming.tick`; `None` when the engine is not arming.
    pub countdown_remaining_ms: Option<u64>,
    pub recording: bool,
    pub last_error: Option<String>,
}

pub struct EngineClient<T: Transport> {
    transport: Option<T>,
    client: Client,
    phase: Phase,
    next_id: u64,
    pending_snapshot_id: Option<u64>,
    view: ViewState,
    /// Consecutive failed connection attempts since the last successful handshake.
    failed_attempts: u32,
    last_pump_ms: u64,
    pub sent: Vec<Frame>,
}

impl<T: Transport> EngineClient<T> {
    pub fn new() -> EngineClient<T> {
        EngineClient { transport: None, client: Client::default(), phase: Phase::Disconnected { next_attempt_ms: 0, attempts: 0 }, next_id: 1, pending_snapshot_id: None, view: ViewState::default(), failed_attempts: 0, last_pump_ms: 0, sent: Vec::new() }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// Attach a transport and send `engine.hello` declaring the consent-surface capabilities.
    pub fn connect(&mut self, transport: T, now_ms: u64) {
        self.transport = Some(transport);
        self.client = Client::default();
        self.view = ViewState::default();
        let hello = Hello {
            client_protocol: PROTOCOL_VERSION,
            client_capabilities: CLIENT_CAPABILITIES.to_vec(),
        };
        self.phase = Phase::Handshaking;
        let id = self.take_id();
        self.send_at(now_ms, Frame::request(
            id,
            "engine.hello",
            Some(serde_json::to_value(hello).expect("hello serializes")),
        ));
    }

    /// Whether a reconnect attempt is due.
    pub fn reconnect_due(&self, now_ms: u64) -> bool {
        matches!(self.phase, Phase::Disconnected { next_attempt_ms, .. } if now_ms >= next_attempt_ms)
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send(&mut self, frame: Frame) {
        let now = self.last_pump_ms;
        self.send_at(now, frame);
    }

    fn send_at(&mut self, now_ms: u64, frame: Frame) {
        self.sent.push(frame.clone());
        let closed = match self.transport.as_mut() {
            Some(t) => t.send(&frame).is_err(),
            None => true,
        };
        if closed {
            self.disconnect(now_ms);
        }
    }

    fn disconnect(&mut self, now_ms: u64) {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        let delay = BACKOFF_MS[((self.failed_attempts - 1) as usize).min(BACKOFF_MS.len() - 1)];
        self.transport = None;
        self.view = ViewState::default();
        self.phase = Phase::Disconnected { next_attempt_ms: now_ms + delay, attempts: self.failed_attempts };
    }

    /// Ask the engine for the authoritative state; local state is discarded until it arrives.
    pub fn request_snapshot(&mut self) {
        self.view.session_state = None;
        self.view.session_id = None;
        self.view.countdown_remaining_ms = None;
        self.view.recording = false;
        self.phase = Phase::Resyncing;
        let id = self.take_id();
        self.pending_snapshot_id = Some(id);
        self.send(Frame::request(id, "session.snapshot", None));
    }

    pub fn cancel_arming(&mut self) {
        let id = self.take_id();
        self.send(Frame::request(id, "session.cancel_arming", None));
    }

    pub fn start(&mut self) {
        let id = self.take_id();
        self.send(Frame::request(id, "session.start", None));
    }

    /// Read every frame the transport holds. Returns what changed for the caller's logging.
    pub fn pump(&mut self, now_ms: u64) -> Vec<ClientAction> {
        self.last_pump_ms = now_ms;
        let mut actions = Vec::new();
        loop {
            let next = match self.transport.as_mut() {
                Some(t) => t.recv(),
                None => return actions,
            };
            match next {
                Ok(Some(frame)) => {
                    let action = self.client.observe(&frame);
                    self.apply(&frame, &action);
                    actions.push(action);
                }
                Ok(None) => return actions,
                Err(TransportError::Closed) | Err(TransportError::Malformed { .. }) => {
                    self.disconnect(now_ms);
                    actions.push(ClientAction::Disconnected(CloseReason::PeerClosed));
                    return actions;
                }
                Err(TransportError::WouldBlock) => return actions,
            }
        }
    }

    fn apply(&mut self, frame: &Frame, action: &ClientAction) {
        match action {
            ClientAction::Response(id) => {
                if let Frame::Response { result, .. } = frame {
                    let is_hello = self.phase == Phase::Handshaking && *id == 1;
                    if is_hello
                        || Some(*id) == self.pending_snapshot_id
                        || result.get("session_snapshot").is_some()
                    {
                        if Some(*id) == self.pending_snapshot_id {
                            self.pending_snapshot_id = None;
                        }
                        self.render_snapshot(result.get("session_snapshot"));
                        self.phase = Phase::Connected;
                        self.failed_attempts = 0;
                    }
                }
                if let Frame::Error { error, .. } = frame {
                    self.view.last_error = Some(error.message.clone());
                }
            }
            ClientAction::Rendered { .. } => {
                if let Frame::Notification { method, params, .. } = frame {
                    self.render_event(method, params);
                }
            }
            ClientAction::ResnapshotRequired { .. } => self.request_snapshot(),
            ClientAction::Suppressed { .. } => {}
            ClientAction::Disconnected(_) => {
                self.disconnect(self.last_pump_ms);
            }
        }
    }

    fn render_snapshot(&mut self, snapshot: Option<&serde_json::Value>) {
        let Some(snapshot) = snapshot else { return };
        let state = snapshot
            .get("state")
            .and_then(|s| s.as_str())
            .map(str::to_string);
        self.view.connected = true;
        self.view.recording = matches!(state.as_deref(), Some("recording") | Some("ending"));
        if state.as_deref() != Some("arming") {
            self.view.countdown_remaining_ms = None;
        }
        self.view.session_state = state;
        self.view.session_id = snapshot
            .get("session_id")
            .and_then(|s| s.as_str())
            .map(str::to_string);
    }

    fn render_event(&mut self, method: &str, params: &serde_json::Value) {
        match method {
            "arming.tick" => {
                self.view.countdown_remaining_ms =
                    params.get("remaining_ms").and_then(|v| v.as_u64())
            }
            "session.transition" => {
                let to = params
                    .get("to")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                self.view.recording = matches!(to.as_deref(), Some("recording") | Some("ending"));
                if to.as_deref() != Some("arming") {
                    self.view.countdown_remaining_ms = None;
                }
                self.view.session_state = to;
            }
            "error" => {
                self.view.last_error = params
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            }
            _ => {}
        }
    }

    /// Wall-clock time passing changes nothing the user sees: the countdown is the engine's.
    pub fn local_time_elapsed(&mut self, _elapsed_ms: u64) {}
}

impl<T: Transport> Default for EngineClient<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_ipc::{
        Connection, ConnectionState, Dispatcher, DuplexEnd, DuplexPair, EventBody, Method,
        RpcError, SessionAuthority, Transition, TransitionCause,
    };

    const SID: &str = "S-1-5-21-1-2-3-1001";

    struct FakeAuthority {
        state: &'static str,
    }
    impl SessionAuthority for FakeAuthority {
        fn snapshot(&self) -> serde_json::Value {
            serde_json::json!({ "state": self.state, "session_id": "01990ce0-1000-7000-8000-000000000042" })
        }
        fn apply(&mut self, method: &Method, _now_ms: u64) -> Result<Vec<Transition>, RpcError> {
            let (from, to) = match (method, self.state) {
                (Method::SessionCancelArming, "arming") => ("arming", "idle"),
                (Method::SessionStart, _) => (self.state, "recording"),
                _ => return Err(RpcError::new(ma_ipc::ErrorCode::Rejected, "not applicable")),
            };
            self.state = to;
            Ok(vec![Transition {
                from: from.into(),
                to: to.into(),
                cause: TransitionCause {
                    kind: "command".into(),
                    refs: vec![],
                },
            }])
        }
    }

    fn engine(
        state: &'static str,
    ) -> (
        Dispatcher<FakeAuthority>,
        Connection<DuplexEnd>,
        EngineClient<DuplexEnd>,
    ) {
        let (server, client_end) = DuplexPair::with_window(64);
        let conn = Connection::accept(server, Some(SID));
        let mut client = EngineClient::new();
        client.connect(client_end, 0);
        (Dispatcher::new(SID, FakeAuthority { state }), conn, client)
    }

    #[test]
    fn handshake_declares_indicator_and_cancel() {
        let (mut d, mut conn, mut client) = engine("idle");
        let Frame::Request { method, params, .. } = &client.sent[0] else {
            panic!()
        };
        assert_eq!(method, "engine.hello");
        let hello: Hello = serde_json::from_value(params.clone().unwrap()).unwrap();
        assert_eq!(
            hello.client_capabilities,
            [Capability::Indicator, Capability::Cancel]
        );
        d.pump(&mut conn, 0);
        client.pump(0);
        assert_eq!(client.phase(), &Phase::Connected);
        assert_eq!(client.view().session_state.as_deref(), Some("idle"));
        assert!(client.view().connected);
        // the engine's own notification is a consent surface: automatic recording needs no client
        let no_client = ma_session::ConsentSurfaces {
            notification_deliverable: true,
            clients: vec![],
        };
        assert!(
            no_client.available(),
            "auto mode arms with no client attached"
        );
        let nothing = ma_session::ConsentSurfaces {
            notification_deliverable: false,
            clients: vec![],
        };
        assert!(
            !nothing.available(),
            "and with no surface at all, nothing starts"
        );
        let this_client = ma_session::ConsentSurfaces {
            notification_deliverable: false,
            clients: vec![ma_session::ClientCapabilities {
                client: "app-ui".into(),
                indicator: true,
                cancel: true,
            }],
        };
        assert!(this_client.available());
    }

    #[test]
    fn countdown_is_driven_by_engine_events() {
        let (mut d, mut conn, mut client) = engine("arming");
        d.pump(&mut conn, 0);
        client.pump(0);
        assert_eq!(client.view().session_state.as_deref(), Some("arming"));
        assert_eq!(
            client.view().countdown_remaining_ms,
            None,
            "no tick yet, no number"
        );
        conn.publish(EventBody::ArmingTick {
            remaining_ms: 10_000,
        });
        conn.flush();
        client.pump(1);
        assert_eq!(client.view().countdown_remaining_ms, Some(10_000));
        client.local_time_elapsed(4_000);
        assert_eq!(
            client.view().countdown_remaining_ms,
            Some(10_000),
            "local time changes nothing"
        );
        conn.publish(EventBody::ArmingTick {
            remaining_ms: 7_000,
        });
        conn.flush();
        client.pump(4);
        assert_eq!(client.view().countdown_remaining_ms, Some(7_000));
        // cancel from the surface: a command to the engine, and the engine's transition clears the countdown
        client.cancel_arming();
        d.pump(&mut conn, 5);
        conn.flush();
        client.pump(5);
        assert_eq!(client.view().session_state.as_deref(), Some("idle"));
        assert_eq!(client.view().countdown_remaining_ms, None);
        assert!(!client.view().recording);
    }

    #[test]
    fn renders_only_engine_state_and_resnapshots_after_gap() {
        let (mut d, mut conn, mut client) = engine("recording");
        d.pump(&mut conn, 0);
        client.pump(0);
        assert!(client.view().recording);
        // the engine drops levels while the UI is slow: a seq gap
        for _ in 0..(ma_ipc::GENERAL_QUEUE + 10) {
            conn.publish(EventBody::CaptureLevel {
                track: "mic".into(),
                rms: -20,
            });
        }
        conn.flush();
        client.pump(1);
        assert_eq!(client.phase(), &Phase::Resyncing);
        assert_eq!(
            client.view().session_state,
            None,
            "nothing rendered from local inference"
        );
        assert!(!client.view().recording);
        assert!(client
            .sent
            .iter()
            .any(|f| matches!(f, Frame::Request { method, .. } if method == "session.snapshot")));
        // drain the rest, then the snapshot arrives
        for _ in 0..8 {
            d.pump(&mut conn, 2);
            conn.flush();
            client.pump(2);
        }
        assert_eq!(client.phase(), &Phase::Connected);
        assert_eq!(client.view().session_state.as_deref(), Some("recording"));
        assert!(client.view().recording);
    }

    #[test]
    fn reconnects_with_backoff_after_disconnect() {
        let (mut d, mut conn, mut client) = engine("idle");
        d.pump(&mut conn, 0);
        client.pump(0);
        conn.close(ma_ipc::CloseReason::Shutdown);
        assert_eq!(conn.state(), ConnectionState::Closed(ma_ipc::CloseReason::Shutdown));
        client.pump(10_000);
        assert!(matches!(client.phase(), Phase::Disconnected { next_attempt_ms: 10_500, attempts: 1 }), "{:?}", client.phase());
        assert_eq!(client.view(), &ViewState::default(), "stale state is not rendered while disconnected");
        assert!(!client.reconnect_due(10_400));
        assert!(client.reconnect_due(10_500));
        // the engine stays down: every failed attempt backs off further, measured on the real clock
        let mut expected_delay = BACKOFF_MS[1];
        let mut now = 10_500;
        for attempt in 2..=7u32 {
            let (dead_server, mut dead_client) = DuplexPair::pair();
            drop(dead_server);
            dead_client.close();
            client.connect(dead_client, now);
            let Phase::Disconnected { next_attempt_ms, attempts } = client.phase().clone() else { panic!("{:?}", client.phase()) };
            assert_eq!(attempts, attempt);
            assert_eq!(next_attempt_ms, now + expected_delay, "attempt {attempt}");
            now = next_attempt_ms;
            expected_delay = BACKOFF_MS[(attempt as usize).min(BACKOFF_MS.len() - 1)];
        }
        // a live engine: full re-handshake, fresh snapshot, and the backoff resets
        let (server, live) = DuplexPair::pair();
        let mut conn = Connection::accept(server, Some(SID));
        client.connect(live, now);
        d.pump(&mut conn, now);
        client.pump(now);
        assert_eq!(client.phase(), &Phase::Connected);
        assert_eq!(client.view().session_state.as_deref(), Some("idle"));
        conn.close(ma_ipc::CloseReason::Shutdown);
        client.pump(now + 1);
        assert!(matches!(client.phase(), Phase::Disconnected { attempts: 1, .. }), "backoff restarts after a successful handshake");
    }
}
