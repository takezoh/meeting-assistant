//! L3 infrastructure crate: the relational projection (contract-store-ownership), relocatable
//! artifact addressing (contract-artifact-addressing) and the deletion path
//! (contract-retention-purge). The database is a projection: the chunk directory is the truth.

pub mod migration;
pub mod purge;
pub mod repo;
pub mod schema;

use std::path::{Path, PathBuf};

pub use migration::{migrate, LATEST_SCHEMA_VERSION, RELEASED_SCHEMA_VERSIONS};
pub use purge::{delete_meeting, purge, PurgeOutcome};
pub use repo::Store;
pub use schema::{Role, TableFamily};

/// The store's own errors.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database schema version {found} is newer than this build supports ({supported}); refusing to open")]
    NewerSchema { found: u32, supported: u32 },
    #[error("write to table `{table}` is outside the {role:?} role's families")]
    RoleViolation { table: String, role: Role },
    #[error("store busy beyond the declared timeout")]
    StoreBusy,
    #[error("LOCALAPPDATA is not set; the database path cannot be resolved")]
    LocalAppDataUnavailable,
    #[error("artifact root {0} is unreachable")]
    RootUnreachable(PathBuf),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Core(#[from] ma_core_types::CoreError),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// The pinned database location: `%LOCALAPPDATA%\MeetingAssistant\db\meeting-assistant.db`.
/// It is not configurable; only artifact roots are.
pub fn pinned_db_path() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or(StoreError::LocalAppDataUnavailable)?;
    Ok(Path::new(&local)
        .join("MeetingAssistant")
        .join("db")
        .join("meeting-assistant.db"))
}
