//! The closed outcome partition (contract-detector-outcome-partition).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Start,
    Continue,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    /// Another adapter's meeting is active with higher or equal precedence.
    LowerPrecedence { active_adapter_id: String },
}

/// Every evaluation lands in exactly one arm.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Determinate { phase: Phase },
    Unknown,
    Inconclusive,
    Conflicting { suppressed: SuppressionReason },
}

impl Outcome {
    pub fn is_determinate_start(&self) -> bool {
        matches!(
            self,
            Outcome::Determinate {
                phase: Phase::Start
            }
        )
    }
}

/// The total match over `(adapter_matched, corroboration_met, competing_active)`. The compiler
/// enforces exhaustiveness; the absence of a determinate outcome never starts capture.
pub fn partition(
    adapter_matched: bool,
    corroboration_met: bool,
    competing_active: Option<&str>,
) -> Outcome {
    match (adapter_matched, corroboration_met, competing_active) {
        (false, _, _) => Outcome::Unknown,
        (true, false, _) => Outcome::Inconclusive,
        (true, true, Some(active)) => Outcome::Conflicting {
            suppressed: SuppressionReason::LowerPrecedence {
                active_adapter_id: active.to_string(),
            },
        },
        (true, true, None) => Outcome::Determinate {
            phase: Phase::Start,
        },
    }
}
