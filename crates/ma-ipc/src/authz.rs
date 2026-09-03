//! Who may speak to the engine, and which engine a client may trust.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthzError {
    /// Only the SID string is recorded.
    SidMismatch {
        sid: String,
    },
    ImpersonationFailed,
}

/// Compare the impersonated client's user SID with the engine's own. Fail closed.
pub fn authorize_client(engine_sid: &str, client_sid: Option<&str>) -> Result<(), AuthzError> {
    match client_sid {
        None => Err(AuthzError::ImpersonationFailed),
        Some(sid) if sid == engine_sid => Ok(()),
        Some(sid) => Err(AuthzError::SidMismatch {
            sid: sid.to_string(),
        }),
    }
}

/// Compiled in; not readable from configuration, environment or command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildChannel {
    Release,
    Development,
}

impl BuildChannel {
    pub const fn compiled() -> BuildChannel {
        if cfg!(feature = "development") {
            BuildChannel::Development
        } else {
            BuildChannel::Release
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    ValidPinnedSigner,
    Unsigned,
    Invalid,
    /// Revocation unreachable, malformed catalogue: a mismatch for a release client.
    Unverifiable,
}

/// What the client learned about the pipe server via `GetNamedPipeServerProcessId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerImage {
    pub image_path: PathBuf,
    pub same_user_sid: bool,
    pub signature: SignatureStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientContext {
    pub installed_engine_path: PathBuf,
    /// The cargo target directory the client image lives under.
    pub own_target_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TamperWarning {
    pub channel: BuildChannel,
    pub image_path: PathBuf,
    pub reason: &'static str,
}

/// Prefix test on normalized separators, so a Windows path is judged the same on every host.
fn under(path: &Path, dir: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let (path, mut dir) = (norm(path), norm(dir));
    if !dir.ends_with('/') {
        dir.push('/');
    }
    path.starts_with(&dir)
}

/// The server-authenticity rule per build channel. Anything not listed is refused.
pub fn verify_server(
    channel: BuildChannel,
    server: &ServerImage,
    ctx: &ClientContext,
) -> Result<(), TamperWarning> {
    let warn = |reason: &'static str| TamperWarning {
        channel,
        image_path: server.image_path.clone(),
        reason,
    };
    let signed = server.signature == SignatureStatus::ValidPinnedSigner;
    let installed = server.image_path == ctx.installed_engine_path;
    match channel {
        BuildChannel::Release => {
            if installed && signed {
                Ok(())
            } else {
                Err(warn(
                    "release client accepts only the signed installed engine",
                ))
            }
        }
        BuildChannel::Development => {
            if installed {
                return if signed {
                    Ok(())
                } else {
                    Err(warn("an installed path must carry a valid signature"))
                };
            }
            if under(&server.image_path, &ctx.own_target_dir) && server.same_user_sid {
                Ok(())
            } else {
                Err(warn(
                    "development client accepts only same-user binaries from its own build tree",
                ))
            }
        }
    }
}
