//! Decisions carry the signal ids and the rule they were derived from; their ids are derived
//! deterministically so a replay is byte-identical.

use crate::outcome::Outcome;
use ma_core_types::{DecisionId, SignalId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Namespace for deterministic decision identifiers.
const DECISION_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6d, 0x61, 0x2d, 0x64, 0x65, 0x74, 0x65, 0x63, 0x74, 0x2d, 0x64, 0x65, 0x63, 0x69, 0x73, 0x6e,
]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub decision_id: DecisionId,
    pub outcome: Outcome,
    pub adapter_id: Option<String>,
    pub subject_key: Option<String>,
    pub rule_id: String,
    pub evidence: Vec<SignalId>,
    pub produced_at_monotonic: u64,
}

impl Decision {
    /// Build a decision whose id is a function of the adapter table version, the rule, the
    /// outcome, the cited evidence and the instant — the same inputs always give the same id.
    pub fn derive(
        table_version: u32,
        outcome: Outcome,
        adapter_id: Option<String>,
        subject_key: Option<String>,
        rule_id: &str,
        evidence: Vec<SignalId>,
        produced_at_monotonic: u64,
    ) -> Decision {
        assert!(
            !evidence.is_empty(),
            "a decision without evidence is a programming error"
        );
        let outcome_json = serde_json::to_string(&outcome).expect("outcome serializes");
        let ids: Vec<String> = evidence.iter().map(|id| id.to_string()).collect();
        let material = format!(
            "{table_version}|{rule_id}|{outcome_json}|{}|{produced_at_monotonic}|{}",
            ids.join(","),
            adapter_id.clone().unwrap_or_default()
        );
        Decision {
            decision_id: DecisionId::from_uuid(Uuid::new_v5(
                &DECISION_NAMESPACE,
                material.as_bytes(),
            )),
            outcome,
            adapter_id,
            subject_key,
            rule_id: rule_id.to_string(),
            evidence,
            produced_at_monotonic,
        }
    }
}

/// A non-fatal note about the pipeline, such as a disabled adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub adapter_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DetectorOutput {
    pub adapter_table_version: u32,
    pub decisions: Vec<Decision>,
    pub diagnostics: Vec<Diagnostic>,
}

impl DetectorOutput {
    /// Canonical serialized form used for byte-identity checks.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("output serializes")
    }
}
