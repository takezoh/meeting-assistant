//! The detection-only browser channel (contract-extension-channel-trust). The engine listens on
//! loopback, authenticates every request by token and pinned extension origin, drops stale or
//! replayed messages, and turns what survives into ordinary `authority: extension` signals that
//! carry host, tab key, audible and meeting-present — never a URL, never a title. The transport is
//! injected so every rejection path is exercised without a browser or a socket.

pub mod auth;
pub mod message;
pub mod server;

pub use auth::{Authenticator, EndpointDescriptor, RejectReason, Token};
pub use message::{ExtensionMessage, MessageError, MAX_HOST_LEN, MAX_TAB_KEY_LEN};
pub use server::{
    Clock, Counters, Request, Response, Server, ServerConfig, SystemClock, FRESHNESS_WINDOW_MS,
    MAX_MESSAGES_PER_SECOND, MAX_QUEUED_SIGNALS,
};
