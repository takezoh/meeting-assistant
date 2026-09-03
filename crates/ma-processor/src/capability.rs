//! Capability declaration and the typed refusal of anything outside it.

use crate::failure::Failure;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessorKind {
    Transcription,
    Diarization,
    Summarization,
}

/// Where the processor executes. Anything native or external must be `Host`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunsIn {
    /// Pure Rust, allocation-bounded; the boundary check forbids native inference here.
    InProcess,
    /// `ma-processor-host`, one child per job.
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub kind: ProcessorKind,
    /// BCP 47 language tags; empty means language-agnostic (summarization of text in any language).
    pub languages: Vec<String>,
    pub needs_gpu: bool,
    pub max_input_seconds: u32,
    pub streaming: bool,
    pub egress_hosts: Vec<String>,
    pub runs_in: RunsIn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorRequest {
    pub kind: ProcessorKind,
    pub language: Option<String>,
    pub input_seconds: u32,
    pub gpu_available: bool,
}

impl Capability {
    /// Refuse before any work: a typed `Unsupported`, never a best-effort attempt.
    pub fn check(&self, request: &ProcessorRequest) -> Result<(), Failure> {
        if request.kind != self.kind {
            return Err(Failure::Unsupported {
                reason: format!(
                    "processor kind is {:?}, request is {:?}",
                    self.kind, request.kind
                ),
            });
        }
        if !self.languages.is_empty() {
            match &request.language {
                Some(lang) if self.languages.iter().any(|l| l.eq_ignore_ascii_case(lang)) => {}
                Some(lang) => {
                    return Err(Failure::Unsupported {
                        reason: format!("language {lang} is not declared"),
                    });
                }
                None => {
                    return Err(Failure::Unsupported {
                        reason: "a language is required".into(),
                    });
                }
            }
        }
        if request.input_seconds > self.max_input_seconds {
            return Err(Failure::Unsupported {
                reason: format!(
                    "input of {} s exceeds max_input_seconds {}",
                    request.input_seconds, self.max_input_seconds
                ),
            });
        }
        if self.needs_gpu && !request.gpu_available {
            return Err(Failure::Unsupported {
                reason: "processor needs a GPU and none is available".into(),
            });
        }
        Ok(())
    }
}
