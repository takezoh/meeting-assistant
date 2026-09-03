//! Endpoint descriptor, per-start token and request authentication.

use ma_secure::acl::SecurityDescriptor;
use ma_secure::Secret;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A 256-bit token minted at every engine start. Deliberately not `Clone`: one copy per engine.
pub struct Token(Secret<Vec<u8>>);

impl Token {
    pub fn generate() -> Token {
        let bytes: [u8; 32] = rand::random();
        Token(Secret::new(bytes.to_vec()))
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Token {
        Token(Secret::new(bytes.to_vec()))
    }
    /// Lowercase hex, the form carried in the descriptor and in the request header.
    pub fn to_hex(&self) -> String {
        self.0.expose().iter().map(|b| format!("{b:02x}")).collect()
    }
    fn matches(&self, presented: &str) -> bool {
        let expected = self.to_hex();
        expected.len() == presented.len()
            && expected
                .bytes()
                .zip(presented.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(***)")
    }
}

/// `%LOCALAPPDATA%\MeetingAssistant\ext\endpoint.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub port: u16,
    pub token: String,
}

impl EndpointDescriptor {
    pub fn path_under(local_app_data: &Path) -> PathBuf {
        local_app_data
            .join("MeetingAssistant")
            .join("ext")
            .join("endpoint.json")
    }

    /// Write the descriptor with the owner-only descriptor that the platform layer applies. The
    /// returned descriptor is what was requested; applying it is the Windows tier's job.
    pub fn write(
        &self,
        local_app_data: &Path,
        owner_sid: &str,
    ) -> std::io::Result<(PathBuf, SecurityDescriptor)> {
        let path = Self::path_under(local_app_data);
        std::fs::create_dir_all(path.parent().expect("descriptor has a parent"))?;
        let security = SecurityDescriptor::owner_only(owner_sid);
        std::fs::write(
            &path,
            serde_json::to_vec(self).expect("descriptor serializes"),
        )?;
        Ok((path, security))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RejectReason {
    MissingToken,
    WrongToken,
    WebOrigin,
    OriginMismatch,
    Malformed,
    StaleSequence,
    StaleObservation,
    RateLimited,
}

/// Token plus pinned extension origin. A request must satisfy both.
#[derive(Debug)]
pub struct Authenticator {
    token: Token,
    pinned_origin: String,
}

impl Authenticator {
    pub fn new(token: Token, pinned_extension_id: &str) -> Authenticator {
        Authenticator {
            token,
            pinned_origin: format!("chrome-extension://{pinned_extension_id}"),
        }
    }

    pub fn token(&self) -> &Token {
        &self.token
    }

    /// Token and origin check. A browser origin is rejected before the token is even compared.
    pub fn check(
        &self,
        origin: Option<&str>,
        presented_token: Option<&str>,
    ) -> Result<(), RejectReason> {
        match origin {
            Some(o) if o.starts_with("http://") || o.starts_with("https://") => {
                return Err(RejectReason::WebOrigin)
            }
            Some(o) if o == self.pinned_origin => {}
            _ => return Err(RejectReason::OriginMismatch),
        }
        match presented_token {
            None => Err(RejectReason::MissingToken),
            Some(t) if self.token.matches(t) => Ok(()),
            Some(_) => Err(RejectReason::WrongToken),
        }
    }
}
