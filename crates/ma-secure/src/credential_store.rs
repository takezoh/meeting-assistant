//! Credential custody: secrets are read on demand from the operating-system credential store
//! under `MeetingAssistant/<purpose>/<account>` and never cached elsewhere. A missing credential
//! is a typed `NeedsAuthentication` that disables the dependent feature with a visible reason.

use crate::secret::Secret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum CredentialError {
    /// No credential is stored for this purpose and account; the feature is disabled.
    NeedsAuthentication { purpose: String, account: String },
    /// The credential store itself cannot be reached (policy, service stopped).
    StoreUnavailable { store: String },
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::NeedsAuthentication { purpose, account } => {
                write!(f, "needs authentication for {purpose}/{account}")
            }
            CredentialError::StoreUnavailable { store } => {
                write!(f, "credential store {store} is unavailable")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

/// Whether a credential-backed feature may run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FeatureStatus {
    Enabled,
    Disabled { reason: CredentialError },
}

pub trait CredentialStore {
    fn store_name(&self) -> &str;
    /// Read the secret for `purpose`/`account` on demand. Never cached by the caller.
    fn read(&self, purpose: &str, account: &str) -> Result<Secret<String>, CredentialError>;

    /// The feature gate: a missing credential disables the feature with its reason surfaced,
    /// never an anonymous fallback.
    fn feature_status(&self, purpose: &str, account: &str) -> FeatureStatus {
        match self.read(purpose, account) {
            Ok(_) => FeatureStatus::Enabled,
            Err(reason) => FeatureStatus::Disabled { reason },
        }
    }
}

/// Canonical entry name in the operating-system store.
pub fn entry_name(purpose: &str, account: &str) -> String {
    format!("MeetingAssistant/{purpose}/{account}")
}

/// Test and development store. The Windows Credential Manager implementation arrives with the
/// first platform unit; product code depends on the trait only.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    entries: BTreeMap<String, String>,
    unavailable: bool,
}

impl InMemoryCredentialStore {
    pub fn with(entries: &[(&str, &str, &str)]) -> Self {
        let mut store = Self::default();
        for (purpose, account, value) in entries {
            store
                .entries
                .insert(entry_name(purpose, account), (*value).to_string());
        }
        store
    }
    pub fn unavailable() -> Self {
        Self {
            entries: BTreeMap::new(),
            unavailable: true,
        }
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn store_name(&self) -> &str {
        "in-memory"
    }
    fn read(&self, purpose: &str, account: &str) -> Result<Secret<String>, CredentialError> {
        if self.unavailable {
            return Err(CredentialError::StoreUnavailable {
                store: self.store_name().to_string(),
            });
        }
        match self.entries.get(&entry_name(purpose, account)) {
            Some(value) => Ok(Secret::new(value.clone())),
            None => Err(CredentialError::NeedsAuthentication {
                purpose: purpose.to_string(),
                account: account.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_credential_is_needs_authentication() {
        let store = InMemoryCredentialStore::with(&[("summarization", "default", "ZZ-TOKEN-ZZ")]);
        assert_eq!(
            store.read("summarization", "default").unwrap().expose(),
            "ZZ-TOKEN-ZZ"
        );
        let err = store.read("export.drive", "take").unwrap_err();
        assert_eq!(
            err,
            CredentialError::NeedsAuthentication {
                purpose: "export.drive".into(),
                account: "take".into()
            }
        );
        assert_eq!(
            store.feature_status("export.drive", "take"),
            FeatureStatus::Disabled {
                reason: err.clone()
            }
        );
        assert_eq!(
            err.to_string(),
            "needs authentication for export.drive/take",
            "the reason names the purpose, never a value"
        );
        let down = InMemoryCredentialStore::unavailable();
        match down.feature_status("summarization", "default") {
            FeatureStatus::Disabled {
                reason: CredentialError::StoreUnavailable { store },
            } => assert_eq!(store, "in-memory"),
            other => {
                panic!("an unavailable store disables the feature naming the store: {other:?}")
            }
        }
        assert_eq!(
            entry_name("summarization", "default"),
            "MeetingAssistant/summarization/default"
        );
    }
}
