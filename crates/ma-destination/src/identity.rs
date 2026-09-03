//! Export identity: the key every remote object is stamped with, and the recorded identity.

use ma_core_types::{ArtifactId, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// `hash(session_id, artifact_id, artifact_version, destination_id, destination_config_hash)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExportKey(pub String);

impl ExportKey {
    pub fn compute(
        session_id: SessionId,
        artifact_id: ArtifactId,
        artifact_version: u32,
        destination_id: &str,
        config_hash: &str,
    ) -> ExportKey {
        let mut h = Sha256::new();
        for part in [
            session_id.to_string(),
            artifact_id.to_string(),
            artifact_version.to_string(),
            destination_id.to_string(),
            config_hash.to_string(),
        ] {
            h.update(part.as_bytes());
            h.update([0]);
        }
        ExportKey(hex::encode(h.finalize()))
    }
}

/// What the destination gave back and what a post-crash lookup must be able to find again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIdentity {
    pub destination_id: String,
    pub remote_id: String,
    /// The export key as stamped on the remote object (app property / external_id).
    pub external_id: String,
    /// A resumable upload session, persisted before the upload completes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumable_session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    pub key: ExportKey,
    pub session_id: SessionId,
    pub artifact_id: ArtifactId,
    pub artifact_version: u32,
    pub destination_id: String,
    pub bytes: u64,
}

impl ExportRequest {
    pub fn new(
        session_id: SessionId,
        artifact_id: ArtifactId,
        artifact_version: u32,
        destination_id: &str,
        config_hash: &str,
        bytes: u64,
    ) -> ExportRequest {
        ExportRequest {
            key: ExportKey::compute(
                session_id,
                artifact_id,
                artifact_version,
                destination_id,
                config_hash,
            ),
            session_id,
            artifact_id,
            artifact_version,
            destination_id: destination_id.to_string(),
            bytes,
        }
    }
}
