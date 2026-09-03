//! `ma-engine`: the per-user background process that is the single authority for session state
//! (contract-process-topology). Phase 0 wires the seams: the instance lock is the control pipe
//! created with FILE_FLAG_FIRST_PIPE_INSTANCE, a second instance exits with EngineAlreadyRunning
//! without touching any session directory, and the supervisor owns update deferral and processor
//! host children. Capture and detection are wired through their seams, not implemented here.

mod supervisor;

use std::process::ExitCode;

pub use supervisor::authority::EngineAuthority;
pub use supervisor::{
    engine_pipe, InstanceLock, LockError, Supervisor, UpdateDisposition, UpdateOffer,
};

/// Exit codes are part of the contract: an operator can tell the two apart from a task log.
pub const EXIT_ENGINE_ALREADY_RUNNING: u8 = 3;
pub const EXIT_UNSUPPORTED_PLATFORM: u8 = 4;

fn main() -> ExitCode {
    let installation_id =
        std::env::var("MA_INSTALLATION_ID").unwrap_or_else(|_| "default".to_string());
    match supervisor::platform_lock(&installation_id) {
        Ok(_guard) => {
            // Phase 0: the process holds the lock and exits. Serving the pipe is the Windows unit's job.
            ExitCode::SUCCESS
        }
        Err(LockError::EngineAlreadyRunning) => ExitCode::from(EXIT_ENGINE_ALREADY_RUNNING),
        Err(LockError::Unsupported) => ExitCode::from(EXIT_UNSUPPORTED_PLATFORM),
    }
}
