//! The local egress audit record — identifiers and counts, never content — and the build-time host
//! inventory it is checked against.

use ma_core_types::ArtifactId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Created,
    Reconciled,
    Failed { class: crate::retry::RetryClass },
    RejectedUndeclaredHost,
}

/// `{when, destination_id, host, artifact_id, bytes, outcome}`: no field can hold content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub at_ms: u64,
    pub destination_id: String,
    pub host: String,
    pub artifact_id: ArtifactId,
    pub bytes: u64,
    pub outcome: AuditOutcome,
}

#[derive(Debug, Deserialize)]
struct InventoryFile {
    #[serde(default)]
    host: Vec<InventoryEntry>,
}

#[derive(Debug, Deserialize)]
struct InventoryEntry {
    host: String,
}

/// The hosts `egress-inventory.toml` declares, embedded at build time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressInventory {
    hosts: Vec<String>,
}

impl EgressInventory {
    pub fn embedded() -> EgressInventory {
        Self::parse(include_str!("../../../egress-inventory.toml"))
            .expect("egress inventory parses")
    }
    pub fn parse(text: &str) -> Result<EgressInventory, String> {
        let file: InventoryFile = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(EgressInventory {
            hosts: file.host.into_iter().map(|h| h.host).collect(),
        })
    }
    pub fn contains(&self, host: &str) -> bool {
        self.hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
    }
    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }
}
