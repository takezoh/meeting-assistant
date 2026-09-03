//! The closed failure taxonomy. `BudgetExceeded` is a warning event, not a failure.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryCause {
    Transient,
    /// Observed: the host was alive and silent past the stall timeout.
    NoProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case")]
pub enum Failure {
    Unsupported {
        reason: String,
    },
    InvalidInput {
        reason: String,
    },
    Retryable {
        after_ms: u64,
        cause: RetryCause,
    },
    Permanent {
        reason: String,
    },
    Cancelled,
    /// Inferred from an abnormal exit status of the host child.
    HostCrashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum Warning {
    BudgetExceeded { budget_ms: u64, elapsed_ms: u64 },
}
