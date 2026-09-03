//! Server-side connection handling and the client-side resync rule.

use crate::authz::{authorize_client, AuthzError};
use crate::event::{Event, EventBody};
use crate::method::Method;
use crate::protocol::{ErrorCode, Frame, Hello, HelloResult, RpcError, PROTOCOL_VERSION};
use crate::transport::{Transport, TransportError};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const OUTBOUND_QUEUE: usize = 256;
pub const TRANSITION_RESERVE: usize = 64;
pub const GENERAL_QUEUE: usize = OUTBOUND_QUEUE - TRANSITION_RESERVE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    ClientTooSlow,
    MalformedFrame { bytes: usize },
    Unauthorized,
    PeerClosed,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Handshaking,
    Open,
    Closed(CloseReason),
}

/// One client connection on the engine side.
pub struct Connection<T: Transport> {
    transport: T,
    state: ConnectionState,
    client_sid: Option<String>,
    next_seq: u64,
    queue: VecDeque<Event>,
    pub dispatched: u64,
    pub dropped_levels: u64,
}

impl<T: Transport> Connection<T> {
    /// `client_sid` is what impersonation reported; `None` means impersonation failed.
    pub fn accept(transport: T, client_sid: Option<&str>) -> Connection<T> {
        Connection {
            transport,
            state: ConnectionState::Handshaking,
            client_sid: client_sid.map(str::to_string),
            next_seq: 0,
            queue: VecDeque::new(),
            dispatched: 0,
            dropped_levels: 0,
        }
    }
    pub fn state(&self) -> ConnectionState {
        self.state
    }
    pub fn last_seq(&self) -> u64 {
        self.next_seq
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
    pub fn close(&mut self, reason: CloseReason) {
        if let ConnectionState::Closed(_) = self.state {
            return;
        }
        if let CloseReason::ClientTooSlow = reason {
            let _ = self.transport.send(&Frame::error(
                None,
                RpcError::new(ErrorCode::ClientTooSlow, "client too slow"),
            ));
        }
        self.transport.close();
        self.state = ConnectionState::Closed(reason);
    }

    /// Queue an event. Never blocks: overflow drops `capture.level` oldest-first, and a transition
    /// that would overflow its reserve closes the connection instead.
    pub fn publish(&mut self, body: EventBody) {
        if let ConnectionState::Closed(_) = self.state {
            return;
        }
        let transitions = self.queue.iter().filter(|e| e.body.is_transition()).count();
        if body.is_transition() {
            if transitions >= TRANSITION_RESERVE {
                self.close(CloseReason::ClientTooSlow);
                return;
            }
        } else {
            let general = self.queue.len() - transitions;
            if general >= GENERAL_QUEUE {
                let victim = self
                    .queue
                    .iter()
                    .position(|e| e.body.is_droppable())
                    .or_else(|| self.queue.iter().position(|e| !e.body.is_transition()));
                if let Some(index) = victim {
                    self.queue.remove(index);
                    self.dropped_levels += 1;
                }
            }
        }
        self.next_seq += 1;
        self.queue.push_back(Event {
            seq: self.next_seq,
            body,
        });
    }

    /// Push queued events to the transport until it would block.
    pub fn flush(&mut self) {
        while let Some(event) = self.queue.front() {
            let frame = Frame::notification(
                event.body.name(),
                serde_json::to_value(event).expect("event serializes"),
            );
            match self.transport.send(&frame) {
                Ok(()) => {
                    self.queue.pop_front();
                }
                Err(TransportError::WouldBlock) => break,
                Err(_) => {
                    self.close(CloseReason::PeerClosed);
                    break;
                }
            }
        }
    }

    fn respond(&mut self, frame: Frame) {
        if self.transport.send(&frame).is_err() {
            self.close(CloseReason::PeerClosed);
        }
    }
}

/// A session transition as the wire sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionCause {
    pub kind: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub cause: TransitionCause,
}

/// The seam through which session semantics reach the wire layer. `ma-engine` implements it over
/// `ma-session`; tests implement it with a fake. Every applied method must be visible in the next
/// `snapshot()`.
pub trait SessionAuthority {
    fn snapshot(&self) -> serde_json::Value;
    fn apply(&mut self, method: &Method, now_ms: u64) -> Result<Vec<Transition>, RpcError>;
}

/// The engine-side dispatcher: one authority, many connections.
pub struct Dispatcher<A: SessionAuthority> {
    pub authority: A,
    pub engine_version: String,
    pub engine_sid: String,
}

impl<A: SessionAuthority> Dispatcher<A> {
    pub fn new(engine_sid: &str, authority: A) -> Dispatcher<A> {
        Dispatcher {
            authority,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            engine_sid: engine_sid.to_string(),
        }
    }

    fn hello(&self, hello: &Hello, last_seq: u64) -> Result<HelloResult, RpcError> {
        if hello.client_protocol.major != PROTOCOL_VERSION.major {
            return Err(RpcError::protocol_mismatch(
                PROTOCOL_VERSION,
                hello.client_protocol,
            ));
        }
        Ok(HelloResult {
            engine_protocol: PROTOCOL_VERSION,
            engine_version: self.engine_version.clone(),
            session_snapshot: self.authority.snapshot(),
            event_seq: last_seq,
        })
    }

    /// Pump one frame from a connection. Authorization happens before any method is looked at.
    pub fn pump<T: Transport>(&mut self, conn: &mut Connection<T>, now_ms: u64) {
        if let ConnectionState::Closed(_) = conn.state {
            return;
        }
        if let Err(AuthzError::SidMismatch { .. } | AuthzError::ImpersonationFailed) =
            authorize_client(&self.engine_sid, conn.client_sid.as_deref())
        {
            conn.close(CloseReason::Unauthorized);
            return;
        }
        let frame = match conn.transport.recv() {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(TransportError::Malformed { bytes }) => {
                conn.close(CloseReason::MalformedFrame { bytes });
                return;
            }
            Err(_) => {
                conn.close(CloseReason::PeerClosed);
                return;
            }
        };
        let Frame::Request {
            id, method, params, ..
        } = frame
        else {
            conn.close(CloseReason::MalformedFrame { bytes: 0 });
            return;
        };
        conn.dispatched += 1;
        let parsed = match Method::parse(&method, params.as_ref()) {
            Ok(m) => m,
            Err(e) => {
                conn.respond(Frame::error(Some(id), e));
                return;
            }
        };
        match (conn.state, &parsed) {
            (ConnectionState::Handshaking, Method::EngineHello(hello)) => {
                match self.hello(hello, conn.next_seq) {
                    Ok(result) => {
                        conn.state = ConnectionState::Open;
                        conn.respond(Frame::response(
                            id,
                            serde_json::to_value(result).expect("hello result serializes"),
                        ));
                    }
                    Err(e) => conn.respond(Frame::error(Some(id), e)),
                }
            }
            (ConnectionState::Handshaking, _) => conn.respond(Frame::error(
                Some(id),
                RpcError::new(ErrorCode::NotHandshaken, "engine.hello first"),
            )),
            (ConnectionState::Open, Method::EngineHello(_)) => conn.respond(Frame::error(
                Some(id),
                RpcError::new(ErrorCode::InvalidRequest, "already handshaken"),
            )),
            (ConnectionState::Open, Method::SessionSnapshot) => {
                let snapshot = serde_json::json!({ "session_snapshot": self.authority.snapshot(), "event_seq": conn.next_seq });
                conn.respond(Frame::response(id, snapshot));
            }
            (ConnectionState::Open, method) => match self.authority.apply(method, now_ms) {
                Ok(transitions) => {
                    for t in transitions {
                        conn.publish(EventBody::SessionTransition {
                            from: t.from,
                            to: t.to,
                            cause: t.cause,
                        });
                    }
                    conn.respond(Frame::response(
                        id,
                        serde_json::json!({ "session_snapshot": self.authority.snapshot() }),
                    ));
                }
                Err(e) => conn.respond(Frame::error(Some(id), e)),
            },
            (ConnectionState::Closed(_), _) => {}
        }
    }
}

/// What a client does with each frame it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAction {
    Rendered {
        seq: u64,
    },
    /// A seq gap: the client must discard local state and call `session.snapshot`.
    ResnapshotRequired {
        expected: u64,
        got: u64,
    },
    /// Received while a re-snapshot is pending: not rendered.
    Suppressed {
        seq: u64,
    },
    Disconnected(CloseReason),
    Response(u64),
}

/// Client-side state: the engine snapshot is the only truth it renders.
#[derive(Debug, Default)]
pub struct Client {
    pub last_seq: Option<u64>,
    pub rendered: Option<serde_json::Value>,
    pub needs_snapshot: bool,
}

impl Client {
    pub fn observe(&mut self, frame: &Frame) -> ClientAction {
        match frame {
            Frame::Notification { params, .. } => {
                let seq = params.get("seq").and_then(|s| s.as_u64()).unwrap_or(0);
                if self.needs_snapshot {
                    self.last_seq = Some(seq);
                    return ClientAction::Suppressed { seq };
                }
                let expected = self.last_seq.map_or(seq, |l| l + 1);
                if seq != expected {
                    self.needs_snapshot = true;
                    self.rendered = None;
                    self.last_seq = Some(seq);
                    return ClientAction::ResnapshotRequired { expected, got: seq };
                }
                self.last_seq = Some(seq);
                ClientAction::Rendered { seq }
            }
            Frame::Error { error, .. } if error.kind() == Some(ErrorCode::ClientTooSlow) => {
                self.rendered = None;
                self.needs_snapshot = true;
                ClientAction::Disconnected(CloseReason::ClientTooSlow)
            }
            Frame::Response { id, result, .. } => {
                if let Some(snapshot) = result.get("session_snapshot") {
                    self.rendered = Some(snapshot.clone());
                    self.needs_snapshot = false;
                    if let Some(seq) = result.get("event_seq").and_then(|s| s.as_u64()) {
                        self.last_seq = Some(seq);
                    }
                }
                ClientAction::Response(*id)
            }
            _ => ClientAction::Response(0),
        }
    }
}
