//! JSON-RPC 2.0 frames, the handshake and the typed error set.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The engine's protocol version. A major mismatch refuses the connection.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion {
    major: 1,
    minor: 0,
    patch: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl ProtocolVersion {
    pub fn parse(text: &str) -> Option<ProtocolVersion> {
        let mut parts = text.split('.').map(|p| p.parse::<u64>().ok());
        let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
        if parts.next().is_some() {
            return None;
        }
        Some(ProtocolVersion {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Serialize for ProtocolVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        ProtocolVersion::parse(&text).ok_or_else(|| serde::de::Error::custom("not a semver triple"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Indicator,
    Cancel,
    Notify,
}

/// `engine.hello` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub client_protocol: ProtocolVersion,
    pub client_capabilities: Vec<Capability>,
}

/// `engine.hello` result: the snapshot is authoritative and `event_seq` is the last seq issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloResult {
    pub engine_protocol: ProtocolVersion,
    pub engine_version: String,
    /// The engine's `SessionState` as JSON; typed on the engine side, opaque on the wire layer.
    pub session_snapshot: serde_json::Value,
    pub event_seq: u64,
}

/// Typed error codes. Application codes sit in the JSON-RPC server-error range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    ProtocolMismatch,
    ClientTooSlow,
    Unauthorized,
    NotHandshaken,
    Rejected,
    /// The method is in the closed set but this build does not perform it yet; never a silent success.
    NotImplemented,
}

impl ErrorCode {
    pub fn code(self) -> i64 {
        match self {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::ProtocolMismatch => -32001,
            ErrorCode::ClientTooSlow => -32002,
            ErrorCode::Unauthorized => -32003,
            ErrorCode::NotHandshaken => -32004,
            ErrorCode::Rejected => -32005,
            ErrorCode::NotImplemented => -32006,
        }
    }
    pub fn from_code(code: i64) -> Option<ErrorCode> {
        [
            ErrorCode::ParseError,
            ErrorCode::InvalidRequest,
            ErrorCode::MethodNotFound,
            ErrorCode::InvalidParams,
            ErrorCode::ProtocolMismatch,
            ErrorCode::ClientTooSlow,
            ErrorCode::Unauthorized,
            ErrorCode::NotHandshaken,
            ErrorCode::Rejected,
            ErrorCode::NotImplemented,
        ]
        .into_iter()
        .find(|c| c.code() == code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> RpcError {
        RpcError {
            code: code.code(),
            message: message.into(),
            data: None,
        }
    }
    pub fn with_data(mut self, data: serde_json::Value) -> RpcError {
        self.data = Some(data);
        self
    }
    pub fn kind(&self) -> Option<ErrorCode> {
        ErrorCode::from_code(self.code)
    }
    pub fn method_not_found(name: &str) -> RpcError {
        RpcError::new(ErrorCode::MethodNotFound, "method not found")
            .with_data(serde_json::json!({ "method": name }))
    }
    /// Names the version the client must speak; nothing else about the connection is touched.
    pub fn protocol_mismatch(required: ProtocolVersion, offered: ProtocolVersion) -> RpcError {
        RpcError::new(ErrorCode::ProtocolMismatch, "protocol major version mismatch").with_data(serde_json::json!({ "required_protocol": required.to_string(), "offered_protocol": offered.to_string() }))
    }
}

/// One JSON-RPC 2.0 frame. Requests carry an id; events are notifications without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Frame {
    Request {
        jsonrpc: Version,
        id: u64,
        method: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<serde_json::Value>,
    },
    Response {
        jsonrpc: Version,
        id: u64,
        result: serde_json::Value,
    },
    Error {
        jsonrpc: Version,
        id: Option<u64>,
        error: RpcError,
    },
    Notification {
        jsonrpc: Version,
        method: String,
        params: serde_json::Value,
    },
}

/// The literal `"2.0"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Version;

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text == "2.0" {
            Ok(Version)
        } else {
            Err(serde::de::Error::custom("jsonrpc must be \"2.0\""))
        }
    }
}

impl Frame {
    pub fn request(id: u64, method: &str, params: Option<serde_json::Value>) -> Frame {
        Frame::Request {
            jsonrpc: Version,
            id,
            method: method.to_string(),
            params,
        }
    }
    pub fn response(id: u64, result: serde_json::Value) -> Frame {
        Frame::Response {
            jsonrpc: Version,
            id,
            result,
        }
    }
    pub fn error(id: Option<u64>, error: RpcError) -> Frame {
        Frame::Error {
            jsonrpc: Version,
            id,
            error,
        }
    }
    pub fn notification(method: &str, params: serde_json::Value) -> Frame {
        Frame::Notification {
            jsonrpc: Version,
            method: method.to_string(),
            params,
        }
    }
}
