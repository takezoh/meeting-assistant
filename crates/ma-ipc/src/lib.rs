//! The engine control channel (contract-ipc-protocol, contract-ipc-transport-authz): JSON-RPC 2.0
//! frames, the closed method and event sets, the version handshake, the reconnect snapshot with
//! per-connection sequence numbers, bounded outbound queues that never stall capture, and the
//! transport authorization rules. The protocol layer is transport generic; `transport::DuplexPair`
//! drives both sides in one test process. This crate is a contract crate (layer L1): session
//! semantics reach it only through the `SessionAuthority` seam that `ma-engine` implements.

pub mod authz;
pub mod dispatch;
pub mod event;
pub mod method;
pub mod protocol;
pub mod transport;

pub use authz::{
    authorize_client, verify_server, AuthzError, BuildChannel, ClientContext, ServerImage,
    SignatureStatus, TamperWarning,
};
pub use dispatch::{
    Client, ClientAction, CloseReason, Connection, ConnectionState, Dispatcher, SessionAuthority,
    Transition, TransitionCause, GENERAL_QUEUE, OUTBOUND_QUEUE, TRANSITION_RESERVE,
};
pub use event::{Event, EventBody};
pub use method::{AppClassName, Method, ModeName, ModeScope, METHOD_NAMES};
pub use protocol::{
    Capability, ErrorCode, Frame, Hello, HelloResult, ProtocolVersion, RpcError, PROTOCOL_VERSION,
};
pub use transport::{DuplexEnd, DuplexPair, Transport, TransportError};
