//! Signed update and adapter manifests (contract-release-manifest-trust). Verification takes bytes
//! and a key set; parsing is only reachable through a verified payload, so no manifest-declared
//! value — URL, path, version, digest, extension id — can influence anything, including a log line,
//! before the Ed25519 signature checks out. Rollback protection, key rollover, digest-gated adapter
//! activation and update deferral are client-side decisions; no server decides what to install.

pub mod keys;
pub mod manifest;
pub mod rollback;
pub mod verify;

pub use keys::{KeyId, KeySet};
pub use manifest::{AdapterManifest, Artifact, KeyRollover, UpdateManifest, DISTRIBUTION_HOSTS};
pub use rollback::{check_version, UpdateDecision, Updater};
pub use verify::{sign, verify, RejectCode, Verified, HEADER_PREFIX};
