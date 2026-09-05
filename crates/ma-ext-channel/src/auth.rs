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

    /// Write the descriptor and apply the owner-only security descriptor to it through `applier`
    /// before the path is returned (contract-extension-trust-reversal-check). The returned
    /// descriptor is the one that was applied.
    pub fn write(
        &self,
        local_app_data: &Path,
        owner_sid: &str,
        applier: &mut dyn AclApplier,
    ) -> std::io::Result<(PathBuf, SecurityDescriptor)> {
        let path = Self::path_under(local_app_data);
        std::fs::create_dir_all(path.parent().expect("descriptor has a parent"))?;
        let security = SecurityDescriptor::owner_only(owner_sid);
        std::fs::write(
            &path,
            serde_json::to_vec(self).expect("descriptor serializes"),
        )?;
        if let Err(error) = applier.apply(&path, &security) {
            // A token file that failed closed must not remain readable with inherited ACLs.
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok((path, security))
    }
}

/// Applies a [`SecurityDescriptor`] to a file. The live implementation is [`WindowsAclApplier`];
/// [`RecordingApplier`] is the portable one, recording every call so a test can assert the
/// descriptor was applied to the path that is returned.
pub trait AclApplier {
    fn apply(&mut self, path: &Path, descriptor: &SecurityDescriptor) -> std::io::Result<()>;
}

/// Records `(path, sddl)` for every application; never touches the file.
#[derive(Debug, Default, Clone)]
pub struct RecordingApplier {
    pub applied: Vec<(PathBuf, String)>,
}

impl AclApplier for RecordingApplier {
    fn apply(&mut self, path: &Path, descriptor: &SecurityDescriptor) -> std::io::Result<()> {
        self.applied
            .push((path.to_path_buf(), descriptor.to_sddl()));
        Ok(())
    }
}

/// Sets the file's owner and protected DACL from the descriptor's SDDL.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsAclApplier;

#[cfg(windows)]
impl AclApplier for WindowsAclApplier {
    fn apply(&mut self, path: &Path, descriptor: &SecurityDescriptor) -> std::io::Result<()> {
        use windows::core::HSTRING;
        use windows::Win32::Foundation::LocalFree;
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::{
            SetFileSecurityW, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        };

        let sddl = HSTRING::from(descriptor.to_sddl());
        let mut psd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: out-pointer for a LocalAlloc'd security descriptor, freed below.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut psd,
                None,
            )
        }
        .map_err(|e| std::io::Error::other(format!("sddl: {e}")))?;
        let name = HSTRING::from(path.as_os_str());
        // SAFETY: valid path string and a converted security descriptor.
        let ok = unsafe {
            SetFileSecurityW(
                &name,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                psd,
            )
        };
        let err = if ok.as_bool() {
            None
        } else {
            Some(std::io::Error::last_os_error())
        };
        // SAFETY: frees what ConvertStringSecurityDescriptorToSecurityDescriptorW allocated.
        unsafe {
            let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(psd.0)));
        }
        match err {
            None => Ok(()),
            Some(e) => Err(e),
        }
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
