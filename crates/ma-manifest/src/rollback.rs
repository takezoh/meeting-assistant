//! Rollback protection, key rollover application, digest-gated adapter activation and update
//! deferral: every decision is made on this machine.

use crate::keys::{parse_hex_key, KeySet};
use crate::manifest::{AdapterManifest, UpdateManifest};
use crate::verify::{verify, RejectCode, Verified};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Strictly greater than installed, unless the user explicitly confirmed a downgrade.
pub fn check_version(
    manifest_version: u64,
    installed: u64,
    downgrade_confirmed: bool,
) -> Result<(), RejectCode> {
    if manifest_version > installed || downgrade_confirmed {
        Ok(())
    } else {
        Err(RejectCode::Downgrade)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Verified, newer, and safe to apply now.
    Apply(UpdateManifest),
    /// Verified and newer, but it replaces the engine while a session is non-terminal.
    Deferred(UpdateManifest),
}

/// The client-side updater. Holds the key set so a rollover can extend it.
pub struct Updater {
    pub keys: KeySet,
    /// 0 when no installed manifest is known: a full verification is still required.
    pub installed_manifest_version: u64,
}

impl Updater {
    pub fn new(keys: KeySet, installed_manifest_version: u64) -> Updater {
        Updater {
            keys,
            installed_manifest_version,
        }
    }

    /// Verify bytes, reject downgrades, apply a rollover signed by a current key, and decide.
    pub fn consider(
        &mut self,
        bytes: &[u8],
        session_non_terminal: bool,
        downgrade_confirmed: bool,
    ) -> Result<UpdateDecision, RejectCode> {
        let verified: Verified<'_> = verify(bytes, &self.keys)?;
        let manifest = verified.parse_update()?;
        check_version(
            manifest.manifest_version,
            self.installed_manifest_version,
            downgrade_confirmed,
        )?;
        if let Some(rollover) = &manifest.key_rollover {
            let next = parse_hex_key(&rollover.next_public_key).ok_or(RejectCode::Malformed)?;
            self.keys.insert(&rollover.next_key_id, &next);
        }
        if manifest.engine_replacement && session_non_terminal {
            return Ok(UpdateDecision::Deferred(manifest));
        }
        Ok(UpdateDecision::Apply(manifest))
    }
}

/// Every declared digest must match the file on disk before an adapter is activated. A mismatch
/// is reported, never repaired.
pub fn activate_adapter(manifest: &AdapterManifest, dir: &Path) -> Result<Vec<String>, RejectCode> {
    let mut activated = Vec::new();
    for artifact in &manifest.artifacts {
        let bytes =
            std::fs::read(dir.join(&artifact.name)).map_err(|_| RejectCode::DigestMismatch)?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != artifact.sha256.to_ascii_lowercase() {
            return Err(RejectCode::DigestMismatch);
        }
        activated.push(artifact.name.clone());
    }
    Ok(activated)
}
