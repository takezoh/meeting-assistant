//! The export seam (contract-destination-export-idempotency, contract-egress-inventory). A
//! destination creates or reconciles exactly one remote object per export key; the exporter commits
//! the intent (carrying the key) before any remote call, looks the recorded identity up before
//! creating, falls back to the destination's external-id lookup after a crash, classifies failures,
//! refuses any host absent from `egress-inventory.toml` before the request, and appends an audit
//! record of identifiers and counts to every attempt. Phase 4 supplies the real destinations.

pub mod audit;
pub mod identity;
pub mod retry;

pub use audit::{AuditOutcome, AuditRecord, EgressInventory};
pub use identity::{ExportKey, ExportRequest, RemoteIdentity};
pub use retry::{
    backoff_with_jitter_ms, classify, ExportEntry, ExportQueue, ExportStatus, RetryClass,
    BACKLOG_CAP, MAX_ATTEMPTS,
};

use ma_core_types::ArtifactId;
use std::collections::BTreeMap;

/// A destination failure as the adapter reports it; `classify` maps it to a retry class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestError {
    Network,
    Http {
        status: u16,
    },
    /// Test seam: the process died after the remote object was created.
    Crashed,
}

/// The destination contract. Every implementation must be able to find its own objects by the
/// export key it stamped on them, because `drive.file`-style scopes see nothing else.
pub trait Destination {
    fn id(&self) -> &str;
    fn host(&self) -> &str;
    fn find_by_external_id(
        &mut self,
        export_key: &ExportKey,
    ) -> Result<Option<RemoteIdentity>, DestError>;
    fn create(&mut self, request: &ExportRequest) -> Result<RemoteIdentity, DestError>;
    fn update(
        &mut self,
        identity: &RemoteIdentity,
        request: &ExportRequest,
    ) -> Result<RemoteIdentity, DestError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentState {
    Intended,
    Committed,
}

/// Persistence port for recorded identities and the intent rows; the engine backs it with the store.
pub trait ExportStore {
    fn recorded_identity(&self, key: &ExportKey) -> Option<RemoteIdentity>;
    fn record_identity(&mut self, key: &ExportKey, identity: &RemoteIdentity);
    fn intent(&mut self, key: &ExportKey, state: IntentState);
    fn intent_state(&self, key: &ExportKey) -> Option<IntentState>;
    fn append_audit(&mut self, record: AuditRecord);
    fn audit(&self) -> Vec<AuditRecord>;
}

#[derive(Debug, Default)]
pub struct MemoryExportStore {
    identities: BTreeMap<String, RemoteIdentity>,
    intents: BTreeMap<String, IntentState>,
    audit: Vec<AuditRecord>,
}

impl ExportStore for MemoryExportStore {
    fn recorded_identity(&self, key: &ExportKey) -> Option<RemoteIdentity> {
        self.identities.get(&key.0).cloned()
    }
    fn record_identity(&mut self, key: &ExportKey, identity: &RemoteIdentity) {
        self.identities.insert(key.0.clone(), identity.clone());
    }
    fn intent(&mut self, key: &ExportKey, state: IntentState) {
        self.intents.insert(key.0.clone(), state);
    }
    fn intent_state(&self, key: &ExportKey) -> Option<IntentState> {
        self.intents.get(&key.0).copied()
    }
    fn append_audit(&mut self, record: AuditRecord) {
        self.audit.push(record);
    }
    fn audit(&self) -> Vec<AuditRecord> {
        self.audit.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    Created(RemoteIdentity),
    Reconciled(RemoteIdentity),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// The host is not in `egress-inventory.toml`; nothing was sent.
    UndeclaredHost {
        host: String,
    },
    Failed {
        class: RetryClass,
        error: DestError,
    },
    Crashed,
}

/// The export procedure over any destination and store.
pub struct Exporter<S: ExportStore> {
    pub store: S,
    pub inventory: EgressInventory,
    /// Test seam: die after the remote create returns, before the identity is recorded.
    pub crash_after_create: bool,
}

impl<S: ExportStore> Exporter<S> {
    pub fn new(store: S) -> Exporter<S> {
        Exporter {
            store,
            inventory: EgressInventory::embedded(),
            crash_after_create: false,
        }
    }

    pub fn export(
        &mut self,
        dest: &mut dyn Destination,
        request: &ExportRequest,
        now_ms: u64,
    ) -> Result<ExportOutcome, ExportError> {
        let (dest_id, host) = (dest.id().to_string(), dest.host().to_string());
        // containment first: an undeclared host is refused before any intent or request
        if !self.inventory.contains(&host) {
            self.audit(
                &dest_id,
                &host,
                request.artifact_id,
                0,
                AuditOutcome::RejectedUndeclaredHost,
                now_ms,
            );
            return Err(ExportError::UndeclaredHost { host });
        }
        // intent before effect: the row carrying the export key is committed before any remote call
        self.store.intent(&request.key, IntentState::Intended);
        let attempt =
            |dest: &mut dyn Destination, store: &mut S| -> Result<ExportOutcome, DestError> {
                // recorded identity → reconcile; else the external-id lookup closes the crash window
                let known = match store.recorded_identity(&request.key) {
                    Some(identity) => Some(identity),
                    None => dest.find_by_external_id(&request.key)?,
                };
                match known {
                    Some(identity) => {
                        let identity = dest.update(&identity, request)?;
                        store.record_identity(&request.key, &identity);
                        Ok(ExportOutcome::Reconciled(identity))
                    }
                    None => {
                        let identity = dest.create(request)?;
                        if self.crash_after_create {
                            return Err(DestError::Crashed);
                        }
                        store.record_identity(&request.key, &identity);
                        Ok(ExportOutcome::Created(identity))
                    }
                }
            };
        match attempt(dest, &mut self.store) {
            Ok(outcome) => {
                self.store.intent(&request.key, IntentState::Committed);
                let audit_outcome = match &outcome {
                    ExportOutcome::Created(_) => AuditOutcome::Created,
                    ExportOutcome::Reconciled(_) => AuditOutcome::Reconciled,
                };
                self.audit(
                    &dest_id,
                    &host,
                    request.artifact_id,
                    request.bytes,
                    audit_outcome,
                    now_ms,
                );
                Ok(outcome)
            }
            Err(DestError::Crashed) => Err(ExportError::Crashed),
            Err(error) => {
                let class = classify(&error);
                self.audit(
                    &dest_id,
                    &host,
                    request.artifact_id,
                    request.bytes,
                    AuditOutcome::Failed { class },
                    now_ms,
                );
                Err(ExportError::Failed { class, error })
            }
        }
    }

    fn audit(
        &mut self,
        dest_id: &str,
        host: &str,
        artifact_id: ArtifactId,
        bytes: u64,
        outcome: AuditOutcome,
        now_ms: u64,
    ) {
        self.store.append_audit(AuditRecord {
            at_ms: now_ms,
            destination_id: dest_id.to_string(),
            host: host.to_string(),
            artifact_id,
            bytes,
            outcome,
        });
    }
}
