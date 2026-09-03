//! The closed event set (engine → UI). Every event carries a strictly increasing per-connection seq.

use crate::dispatch::TransitionCause;
use serde::{Deserialize, Serialize};

pub const EVENT_NAMES: [&str; 6] = [
    "session.transition",
    "capture.level",
    "capture.degraded",
    "arming.tick",
    "detector.decision",
    "error",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    #[serde(flatten)]
    pub body: EventBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum EventBody {
    #[serde(rename = "session.transition")]
    SessionTransition {
        /// snake_case names of `ma_session::State`; the schema enumerates them.
        from: String,
        to: String,
        cause: TransitionCause,
    },
    #[serde(rename = "capture.level")]
    /// `rms` in dBFS, per the contract's `capture.level{seq, track, rms}`.
    CaptureLevel { track: String, rms: i16 },
    #[serde(rename = "capture.degraded")]
    CaptureDegraded { reason: String },
    #[serde(rename = "arming.tick")]
    ArmingTick { remaining_ms: u64 },
    #[serde(rename = "detector.decision")]
    DetectorDecision {
        outcome: String,
        evidence: Vec<String>,
    },
    #[serde(rename = "error")]
    Error { code: i64, message: String },
}

impl EventBody {
    pub fn name(&self) -> &'static str {
        match self {
            EventBody::SessionTransition { .. } => "session.transition",
            EventBody::CaptureLevel { .. } => "capture.level",
            EventBody::CaptureDegraded { .. } => "capture.degraded",
            EventBody::ArmingTick { .. } => "arming.tick",
            EventBody::DetectorDecision { .. } => "detector.decision",
            EventBody::Error { .. } => "error",
        }
    }
    pub fn is_transition(&self) -> bool {
        matches!(self, EventBody::SessionTransition { .. })
    }
    pub fn is_droppable(&self) -> bool {
        matches!(self, EventBody::CaptureLevel { .. })
    }
}
