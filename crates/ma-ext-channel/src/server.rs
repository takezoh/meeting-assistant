//! The channel server over an injected transport: authentication, freshness, replay, rate and
//! backlog limits, and conversion of accepted messages into signals.

use crate::auth::{Authenticator, RejectReason};
use crate::message::ExtensionMessage;
use ma_core_types::id::TypedId;
use ma_core_types::SignalId;
use ma_signal::{Authority, ObservedAt, Payload, Signal, SignalKind, Subject, SCHEMA_VERSION};
use std::collections::{BTreeMap, VecDeque};

pub const FRESHNESS_WINDOW_MS: i64 = 5_000;
pub const MAX_MESSAGES_PER_SECOND: usize = 20;
pub const MAX_QUEUED_SIGNALS: usize = 200;
pub const SOURCE_ID: &str = "ext.tabs";
/// Extension instances whose sequence space is remembered (a browser restart opens a new one).
pub const MAX_INSTANCES: usize = 16;

pub trait Clock {
    fn monotonic_ns(&self) -> u64;
    fn wall_utc_ms(&self) -> i64;
}

pub struct SystemClock {
    start: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock {
            start: std::time::Instant::now(),
        }
    }
}

impl SystemClock {
    /// A clock whose monotonic zero is `start`, shared with the other sources of a session.
    pub fn with_origin(start: std::time::Instant) -> Self {
        SystemClock { start }
    }
}

impl Clock for SystemClock {
    fn monotonic_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
    fn wall_utc_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// What the transport hands the server: the parts of an HTTP request the contract looks at.
#[derive(Debug, Clone)]
pub struct Request {
    pub connection_id: u32,
    pub origin: Option<String>,
    pub token: Option<String>,
    pub body: Vec<u8>,
    /// The process-tree root of the browser that opened the connection, as the transport observed
    /// it on the loopback peer; copied into every tab signal so the detector can join tab and
    /// microphone facts (contract-extension-trust-reversal-check). Additive: `None` when the
    /// transport cannot attribute the peer.
    pub peer_process_tree_root_pid: Option<u32>,
}

/// Status only. There is never a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
}

impl Response {
    pub const ACCEPTED: Response = Response { status: 204 };
    pub fn for_rejection(reason: RejectReason) -> Response {
        Response {
            status: match reason {
                RejectReason::MissingToken | RejectReason::WrongToken => 401,
                RejectReason::WebOrigin | RejectReason::OriginMismatch => 403,
                RejectReason::Malformed => 400,
                RejectReason::StaleSequence | RejectReason::StaleObservation => 409,
                RejectReason::RateLimited => 429,
            },
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counters {
    pub accepted: u64,
    pub rejected: BTreeMap<RejectReason, u64>,
    pub dropped_oldest: u64,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub pinned_extension_id: String,
}

pub struct Server<C: Clock> {
    auth: Authenticator,
    clock: C,
    /// Last accepted seq per extension instance; bounded, oldest instance evicted.
    last_seq: BTreeMap<String, u64>,
    instance_order: VecDeque<String>,
    /// Accepted requests in the current server-wide one-second window. The concrete transport
    /// closes every HTTP connection, so a per-connection bucket would be bypassable by reconnecting.
    recent_requests: VecDeque<u64>,
    queue: VecDeque<Signal>,
    counters: Counters,
}

impl<C: Clock> Server<C> {
    /// A new server mints a new token; the token of any previous engine start is dead.
    pub fn start(config: &ServerConfig, clock: C) -> Server<C> {
        Server {
            auth: Authenticator::new(crate::auth::Token::generate(), &config.pinned_extension_id),
            clock,
            last_seq: BTreeMap::new(),
            instance_order: VecDeque::new(),
            recent_requests: VecDeque::new(),
            queue: VecDeque::new(),
            counters: Counters::default(),
        }
    }

    pub fn authenticator(&self) -> &Authenticator {
        &self.auth
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    /// Signals accepted so far, oldest first. Draining empties the queue.
    pub fn drain(&mut self) -> Vec<Signal> {
        self.queue.drain(..).collect()
    }

    /// Handle one request: authenticate, rate-limit, parse, check freshness and sequence, enqueue.
    pub fn handle(&mut self, request: Request) -> Response {
        if let Err(reason) = self
            .auth
            .check(request.origin.as_deref(), request.token.as_deref())
        {
            return self.reject(reason);
        }
        let now_ns = self.clock.monotonic_ns();
        if self.rate_limited(now_ns) {
            return self.reject(RejectReason::RateLimited);
        }
        let message = match ExtensionMessage::parse(&request.body) {
            Ok(m) => m,
            Err(_) => return self.reject(RejectReason::Malformed),
        };
        if self
            .last_seq
            .get(&message.instance_id)
            .is_some_and(|last| message.seq <= *last)
        {
            return self.reject(RejectReason::StaleSequence);
        }
        if self
            .clock
            .wall_utc_ms()
            .saturating_sub(message.observed_at_ms)
            > FRESHNESS_WINDOW_MS
        {
            return self.reject(RejectReason::StaleObservation);
        }
        self.remember_seq(&message.instance_id, message.seq);
        self.counters.accepted += 1;
        for signal in self.signals_for(&message, request.peer_process_tree_root_pid) {
            self.enqueue(signal);
        }
        Response::ACCEPTED
    }

    fn reject(&mut self, reason: RejectReason) -> Response {
        *self.counters.rejected.entry(reason).or_insert(0) += 1;
        Response::for_rejection(reason)
    }

    fn remember_seq(&mut self, instance_id: &str, seq: u64) {
        if !self.last_seq.contains_key(instance_id) {
            self.instance_order.push_back(instance_id.to_string());
            while self.instance_order.len() > MAX_INSTANCES {
                if let Some(old) = self.instance_order.pop_front() {
                    self.last_seq.remove(&old);
                }
            }
        }
        self.last_seq.insert(instance_id.to_string(), seq);
    }

    fn rate_limited(&mut self, now_ns: u64) -> bool {
        while self
            .recent_requests
            .front()
            .is_some_and(|t| now_ns.saturating_sub(*t) >= 1_000_000_000)
        {
            self.recent_requests.pop_front();
        }
        if self.recent_requests.len() >= MAX_MESSAGES_PER_SECOND {
            return true;
        }
        self.recent_requests.push_back(now_ns);
        false
    }

    fn enqueue(&mut self, signal: Signal) {
        if self.queue.len() >= MAX_QUEUED_SIGNALS {
            self.queue.pop_front();
            self.counters.dropped_oldest += 1;
        }
        self.queue.push_back(signal);
    }

    /// `meeting_present` yields `tab_meeting_present`; `audible` yields `tab_audible`; a report with
    /// neither is a keep-alive and yields no signal, so it can never corroborate anything.
    fn signals_for(
        &self,
        message: &ExtensionMessage,
        peer_process_tree_root_pid: Option<u32>,
    ) -> Vec<Signal> {
        let observed_at = ObservedAt {
            monotonic_ns: self.clock.monotonic_ns(),
            wall_utc_ms: self.clock.wall_utc_ms(),
        };
        let subject = Subject::Tab {
            host: message.host.clone(),
            tab_key: message.tab_key.clone(),
        };
        let make = |kind: SignalKind| Signal {
            signal_id: SignalId::new(),
            source_id: SOURCE_ID.to_string(),
            kind,
            subject: subject.clone(),
            observed_at,
            payload: Payload {
                audible: Some(message.audible),
                process_tree_root_pid: peer_process_tree_root_pid,
                ..Payload::default()
            },
            authority: Authority::Extension,
            schema_version: SCHEMA_VERSION,
        };
        let mut out = Vec::new();
        if message.meeting_present {
            out.push(make(SignalKind::TabMeetingPresent));
        }
        if message.audible {
            out.push(make(SignalKind::TabAudible));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::EndpointDescriptor;
    use std::cell::Cell;

    struct FakeClock {
        mono_ns: Cell<u64>,
        wall_ms: Cell<i64>,
    }
    impl Clock for FakeClock {
        fn monotonic_ns(&self) -> u64 {
            self.mono_ns.get()
        }
        fn wall_utc_ms(&self) -> i64 {
            self.wall_ms.get()
        }
    }
    fn server() -> Server<FakeClock> {
        Server::start(
            &ServerConfig {
                pinned_extension_id: "abcdefghijklmnopabcdefghijklmnop".into(),
            },
            FakeClock {
                mono_ns: Cell::new(1_000_000_000),
                wall_ms: Cell::new(1_756_857_600_000),
            },
        )
    }
    const ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
    fn body(seq: u64, observed_at_ms: i64) -> Vec<u8> {
        body_for("inst-a", seq, observed_at_ms, true, true)
    }
    fn body_for(
        instance: &str,
        seq: u64,
        observed_at_ms: i64,
        audible: bool,
        meeting_present: bool,
    ) -> Vec<u8> {
        format!(r#"{{"instance_id":"{instance}","seq":{seq},"observed_at_ms":{observed_at_ms},"host":"meet.example.test","tab_key":"tab-17","audible":{audible},"meeting_present":{meeting_present}}}"#).into_bytes()
    }
    fn good(server: &Server<FakeClock>, seq: u64) -> Request {
        Request {
            connection_id: 1,
            origin: Some(ORIGIN.into()),
            token: Some(server.authenticator().token().to_hex()),
            body: body(seq, server.clock.wall_utc_ms()),
            peer_process_tree_root_pid: None,
        }
    }

    #[test]
    fn request_without_token_rejected() {
        let mut s = server();
        let mut r = good(&s, 1);
        r.token = None;
        assert_eq!(s.handle(r), Response { status: 401 });
        let mut r = good(&s, 1);
        r.token = Some("00".repeat(32));
        assert_eq!(s.handle(r), Response { status: 401 });
        assert!(
            s.drain().is_empty(),
            "no signal from an unauthenticated request"
        );
        assert_eq!(s.counters().rejected[&RejectReason::MissingToken], 1);
        assert_eq!(s.counters().rejected[&RejectReason::WrongToken], 1);
    }

    #[test]
    fn web_origin_rejected() {
        let mut s = server();
        for origin in [
            "https://evil.example.test",
            "http://127.0.0.1:4242",
            "chrome-extension://someotherextensionidentifier0",
        ] {
            let mut r = good(&s, 1);
            r.origin = Some(origin.into());
            assert_eq!(s.handle(r).status, 403, "{origin}");
        }
        let mut r = good(&s, 1);
        r.origin = None;
        assert_eq!(
            s.handle(r).status,
            403,
            "no origin is not the pinned origin"
        );
        assert!(s.drain().is_empty());
        assert_eq!(s.counters().rejected[&RejectReason::WebOrigin], 2);
        assert_eq!(s.counters().rejected[&RejectReason::OriginMismatch], 2);
    }

    #[test]
    fn stale_sequence_rejected() {
        let mut s = server();
        assert_eq!(s.handle(good(&s, 5)), Response::ACCEPTED);
        assert_eq!(s.handle(good(&s, 5)).status, 409, "replay of the same seq");
        assert_eq!(s.handle(good(&s, 4)).status, 409, "older seq");
        let mut old = good(&s, 6);
        old.body = body(6, s.clock.wall_utc_ms() - FRESHNESS_WINDOW_MS - 1);
        assert_eq!(
            s.handle(old).status,
            409,
            "observation older than the freshness window"
        );
        assert_eq!(s.handle(good(&s, 6)), Response::ACCEPTED);
        let signals = s.drain();
        assert_eq!(signals.len(), 4, "two accepted messages, two signals each");
        assert_eq!(s.counters().rejected[&RejectReason::StaleSequence], 2);
        assert_eq!(s.counters().rejected[&RejectReason::StaleObservation], 1);
    }

    #[test]
    fn sequence_is_per_instance_and_a_restart_is_not_a_replay() {
        let mut s = server();
        let mut a = good(&s, 500);
        a.body = body_for("inst-a", 500, s.clock.wall_utc_ms(), true, true);
        assert_eq!(s.handle(a), Response::ACCEPTED);
        // the browser restarts: instance b starts at seq 1 and must not be treated as stale
        let mut b = good(&s, 1);
        b.body = body_for("inst-b", 1, s.clock.wall_utc_ms(), true, true);
        assert_eq!(s.handle(b), Response::ACCEPTED);
        let mut a_replay = good(&s, 500);
        a_replay.body = body_for("inst-a", 500, s.clock.wall_utc_ms(), true, true);
        assert_eq!(
            s.handle(a_replay).status,
            409,
            "a replay within the same instance is still stale"
        );
        let mut bad = good(&s, 2);
        bad.body = body_for("bad id!", 2, s.clock.wall_utc_ms(), true, true);
        assert_eq!(s.handle(bad).status, 400);
    }

    #[test]
    fn keep_alive_and_inaudible_reports_cannot_corroborate() {
        let mut s = server();
        let mut r = good(&s, 1);
        r.body = body_for("inst-a", 1, s.clock.wall_utc_ms(), false, false);
        assert_eq!(s.handle(r), Response::ACCEPTED);
        assert!(
            s.drain().is_empty(),
            "a landing page report yields no signal at all"
        );
        let mut r = good(&s, 2);
        r.body = body_for("inst-a", 2, s.clock.wall_utc_ms(), true, false);
        assert_eq!(s.handle(r), Response::ACCEPTED);
        let signals = s.drain();
        assert_eq!(
            signals.iter().map(|x| x.kind).collect::<Vec<_>>(),
            [SignalKind::TabAudible],
            "audible without a meeting is not tab_meeting_present"
        );
    }

    #[test]
    fn accepted_message_carries_host_and_tab_key_only() {
        let mut s = server();
        assert_eq!(s.handle(good(&s, 1)), Response::ACCEPTED);
        let signals = s.drain();
        for signal in &signals {
            assert_eq!(signal.authority, Authority::Extension);
            assert_eq!(
                signal.subject,
                Subject::Tab {
                    host: "meet.example.test".into(),
                    tab_key: "tab-17".into()
                }
            );
            assert_eq!(signal.payload.audible, Some(true));
            let json = serde_json::to_string(signal).unwrap();
            assert!(!json.contains("://") && !json.contains("title"), "{json}");
        }
        assert_eq!(
            signals.iter().map(|s| s.kind).collect::<Vec<_>>(),
            [SignalKind::TabMeetingPresent, SignalKind::TabAudible]
        );
    }

    #[test]
    fn descriptor_is_owner_only_and_token_rotates_per_start() {
        let dir = tempfile::tempdir().unwrap();
        let a = server();
        let b = server();
        assert_ne!(
            a.authenticator().token().to_hex(),
            b.authenticator().token().to_hex()
        );
        let descriptor = EndpointDescriptor {
            port: 49_152,
            token: a.authenticator().token().to_hex(),
        };
        let mut applier = crate::auth::RecordingApplier::default();
        let (path, security) = descriptor
            .write(dir.path(), "S-1-5-21-1-2-3-1001", &mut applier)
            .unwrap();
        assert!(path.ends_with("MeetingAssistant/ext/endpoint.json"));
        assert!(security.grants_owner_only());
        assert_eq!(
            applier.applied.len(),
            1,
            "the descriptor is applied, not only built"
        );
        let read: EndpointDescriptor =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read, descriptor);
        assert_eq!(format!("{:?}", a.authenticator().token()), "Token(***)");
    }

    #[test]
    fn flood_is_rate_limited_and_backlog_drops_oldest() {
        let mut s = server();
        let mut accepted = 0;
        for seq in 1..=25 {
            if s.handle(good(&s, seq)) == Response::ACCEPTED {
                accepted += 1;
            }
        }
        assert_eq!(accepted, MAX_MESSAGES_PER_SECOND);
        assert_eq!(s.counters().rejected[&RejectReason::RateLimited], 5);
        let mut seq = 100;
        for _ in 0..7 {
            s.clock.mono_ns.set(s.clock.mono_ns.get() + 1_000_000_000);
            for _ in 0..MAX_MESSAGES_PER_SECOND {
                seq += 1;
                s.handle(good(&s, seq));
            }
        }
        assert_eq!(s.queue.len(), MAX_QUEUED_SIGNALS);
        assert!(s.counters().dropped_oldest > 0);
    }

    #[test]
    fn reconnecting_does_not_bypass_the_server_wide_rate_limit() {
        let mut s = server();
        for seq in 1..=MAX_MESSAGES_PER_SECOND as u64 {
            let mut request = good(&s, seq);
            request.connection_id = seq as u32;
            assert_eq!(s.handle(request), Response::ACCEPTED);
        }
        let mut request = good(&s, MAX_MESSAGES_PER_SECOND as u64 + 1);
        request.connection_id = 10_000;
        assert_eq!(s.handle(request).status, 429);
        assert_eq!(s.recent_requests.len(), MAX_MESSAGES_PER_SECOND);
    }
}
