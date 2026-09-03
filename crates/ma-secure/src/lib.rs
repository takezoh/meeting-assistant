//! L1 contract crate: secrets exist in exactly one place (contract-credential-custody), logs
//! cannot carry meeting content by type (contract-diagnostic-redaction), and pipe and file
//! security descriptors grant the owning user only (contract-ipc-transport-authz). The egress
//! inventory check lives in `tests/egress_inventory.rs` and reads `egress-inventory.toml`.

pub mod acl;
pub mod credential_store;
pub mod redaction;
pub mod secret;

pub use acl::{AccessMask, Ace, PipeSecurity, SecurityDescriptor};
pub use credential_store::{
    CredentialError, CredentialStore, FeatureStatus, InMemoryCredentialStore,
};
pub use redaction::{Content, LogField, LogValue, ParseError, RedactedPath};
pub use secret::Secret;
