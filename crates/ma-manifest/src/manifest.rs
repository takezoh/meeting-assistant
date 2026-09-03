//! Manifest payloads. These types are only constructible from a `Verified` payload.

use serde::{Deserialize, Serialize};

/// The only hosts a manifest may point at; a manifest naming any other host is malformed.
pub const DISTRIBUTION_HOSTS: [&str; 2] = ["github.com", "objects.githubusercontent.com"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyRollover {
    pub next_key_id: String,
    pub next_public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateManifest {
    /// Strictly increasing integer; distinct from the display version.
    pub manifest_version: u64,
    pub version: String,
    pub channel: String,
    pub artifacts: Vec<Artifact>,
    /// The update replaces the engine binary and therefore waits for a terminal session.
    pub engine_replacement: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_rollover: Option<KeyRollover>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    pub manifest_version: u64,
    pub adapter_id: String,
    pub version: String,
    pub artifacts: Vec<Artifact>,
    /// The pinned browser extension identifier for the detection-only channel, when the adapter has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_extension_id: Option<String>,
}

pub(crate) fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    Some(rest.split('/').next().unwrap_or(rest))
}

pub(crate) fn artifacts_well_formed(artifacts: &[Artifact]) -> bool {
    artifacts.iter().all(|a| {
        host_of(&a.url).is_some_and(|h| DISTRIBUTION_HOSTS.contains(&h))
            && a.sha256.len() == 64
            && a.sha256.chars().all(|c| c.is_ascii_hexdigit())
            && !a.name.contains('/')
            && !a.name.contains('\\')
            && !a.name.contains("..")
    })
}
