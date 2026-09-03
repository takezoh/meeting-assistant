//! Export family repository (writer: engine) and the identifier-only export payload skeleton.

use crate::repo::Store;
use crate::Result;
use ma_core_types::{ExportId, MeetingId, SessionId};
use serde::{Deserialize, Serialize};

/// An export identifies its meeting by id, never by title (contract-stable-identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPayload {
    pub meeting_id: MeetingId,
    pub session_id: SessionId,
}

impl ExportPayload {
    pub fn new(meeting_id: MeetingId, session_id: SessionId) -> Self {
        Self {
            meeting_id,
            session_id,
        }
    }
}

pub fn insert_export(
    store: &mut Store,
    export: ExportId,
    meeting: MeetingId,
    destination: &str,
    remote_id: Option<&str>,
) -> Result<()> {
    store.conn().execute("INSERT INTO export (export_id, meeting_id, destination, status, remote_id) VALUES (?1, ?2, ?3, 'committed', ?4)", (export.to_string(), meeting.to_string(), destination, remote_id))?;
    Ok(())
}

/// One row per outbound send: `{when, destination_id, host, artifact_id, bytes, outcome}` plus the
/// purpose and the remote reference. Identifiers and counts only — never content.
#[allow(clippy::too_many_arguments)]
pub fn record_egress(
    store: &mut Store,
    meeting: Option<MeetingId>,
    destination_id: &str,
    host: &str,
    purpose: &str,
    artifact_id: Option<ma_core_types::ArtifactId>,
    bytes: u64,
    remote_ref: Option<&str>,
    outcome: &str,
    at_ms: i64,
) -> Result<()> {
    store.conn().execute("INSERT INTO egress_audit (meeting_id, destination_id, host, purpose, artifact_id, bytes, remote_ref, outcome, at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", (meeting.map(|m| m.to_string()), destination_id, host, purpose, artifact_id.map(|a| a.to_string()), bytes as i64, remote_ref, outcome, at_ms))?;
    Ok(())
}
