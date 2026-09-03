//! The closed method set (UI → engine). Every method's effect is observable in a later snapshot.

use crate::protocol::{Hello, RpcError};
use ma_core_types::{ArtifactId, MeetingId};
use serde::{Deserialize, Serialize};

pub const METHOD_NAMES: [&str; 14] = [
    "engine.hello",
    "session.snapshot",
    "session.start",
    "session.stop",
    "session.pause",
    "session.resume",
    "session.discard",
    "session.cancel_arming",
    "session.extend_hysteresis",
    "mode.set",
    "artifact.edit",
    "meeting.delete",
    "diagnostics.export",
    "engine.shutdown",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Method {
    #[serde(rename = "engine.hello")]
    EngineHello(Hello),
    #[serde(rename = "session.snapshot")]
    SessionSnapshot,
    #[serde(rename = "session.start")]
    SessionStart,
    #[serde(rename = "session.stop")]
    SessionStop,
    #[serde(rename = "session.pause")]
    SessionPause,
    #[serde(rename = "session.resume")]
    SessionResume,
    #[serde(rename = "session.discard")]
    SessionDiscard,
    #[serde(rename = "session.cancel_arming")]
    SessionCancelArming,
    #[serde(rename = "session.extend_hysteresis")]
    SessionExtendHysteresis { extend: bool },
    #[serde(rename = "mode.set")]
    ModeSet { scope: ModeScope, mode: ModeName },
    #[serde(rename = "artifact.edit")]
    ArtifactEdit {
        artifact_id: ArtifactId,
        edit_revision: u32,
    },
    #[serde(rename = "meeting.delete")]
    MeetingDelete { meeting_id: MeetingId },
    #[serde(rename = "diagnostics.export")]
    DiagnosticsExport,
    #[serde(rename = "engine.shutdown")]
    EngineShutdown,
}

/// Wire form of `ma_session::Mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeName {
    Auto,
    Ask,
    Manual,
}

/// Wire form of `ma_session::AppClass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppClassName {
    Desktop,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModeScope {
    Global,
    Class { class: AppClassName },
    Adapter { adapter_id: String },
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::EngineHello(_) => "engine.hello",
            Method::SessionSnapshot => "session.snapshot",
            Method::SessionStart => "session.start",
            Method::SessionStop => "session.stop",
            Method::SessionPause => "session.pause",
            Method::SessionResume => "session.resume",
            Method::SessionDiscard => "session.discard",
            Method::SessionCancelArming => "session.cancel_arming",
            Method::SessionExtendHysteresis { .. } => "session.extend_hysteresis",
            Method::ModeSet { .. } => "mode.set",
            Method::ArtifactEdit { .. } => "artifact.edit",
            Method::MeetingDelete { .. } => "meeting.delete",
            Method::DiagnosticsExport => "diagnostics.export",
            Method::EngineShutdown => "engine.shutdown",
        }
    }

    /// Resolve a request's method name and params. Unknown names are a typed error, never a no-op.
    pub fn parse(name: &str, params: Option<&serde_json::Value>) -> Result<Method, RpcError> {
        if !METHOD_NAMES.contains(&name) {
            return Err(RpcError::method_not_found(name));
        }
        let mut envelope = serde_json::json!({ "method": name });
        if let Some(p) = params {
            envelope["params"] = p.clone();
        }
        serde_json::from_value(envelope)
            .map_err(|_| RpcError::new(crate::protocol::ErrorCode::InvalidParams, "invalid params"))
    }

    pub fn params(&self) -> Option<serde_json::Value> {
        let value = serde_json::to_value(self).expect("method serializes");
        value.get("params").cloned()
    }
}
