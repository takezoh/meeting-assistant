//! Forward-only migrations carried by SQLite `user_version`. A database newer than this build
//! is refused with a typed error; it is never read best-effort.

use crate::{Result, StoreError};
use rusqlite::Connection;

/// Ordered migrations; each runs in one transaction and bumps `user_version`.
pub const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

pub const LATEST_SCHEMA_VERSION: u32 = 1;

/// Schema versions that shipped in a release. Adding a release adds an entry here and the
/// forward-migration test gains a case; today no release exists.
pub const RELEASED_SCHEMA_VERSIONS: &[u32] = &[];

pub fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as u32)
}

/// Apply every migration above the current `user_version`, forward only.
pub fn migrate(conn: &Connection) -> Result<u32> {
    let current = user_version(conn)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: LATEST_SCHEMA_VERSION,
        });
    }
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        // one migration, one transaction: a crash leaves either the old or the new version
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let applied = conn
            .execute_batch(sql)
            .and_then(|_| conn.pragma_update(None, "user_version", *version as i64));
        match applied {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(err) => {
                conn.execute_batch("ROLLBACK")?;
                return Err(err.into());
            }
        }
    }
    Ok(LATEST_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Store;
    use crate::schema::{Role, TABLES};

    #[test]
    fn migrate_from_every_released_version() {
        // from empty and from every released version, migrations land on the latest schema
        for &start in [0u32].iter().chain(RELEASED_SCHEMA_VERSIONS.iter()) {
            let dir = tempfile::tempdir().unwrap();
            let conn = Connection::open(dir.path().join(crate::repo::DB_FILE_NAME)).unwrap();
            if start > 0 {
                for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= start) {
                    conn.execute_batch(sql).unwrap();
                    conn.pragma_update(None, "user_version", *version as i64)
                        .unwrap();
                }
            }
            assert_eq!(
                migrate(&conn).unwrap(),
                LATEST_SCHEMA_VERSION,
                "from version {start}"
            );
            assert_eq!(user_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
            for (table, _) in TABLES {
                let exists: i64 = conn
                    .query_row(
                        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                        [table],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(
                    exists, 1,
                    "table {table} exists after migrating from {start}"
                );
            }
            assert_eq!(
                migrate(&conn).unwrap(),
                LATEST_SCHEMA_VERSION,
                "migrate is idempotent"
            );
        }
        // a newer database is refused with a typed error naming the versions
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::repo::DB_FILE_NAME);
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", (LATEST_SCHEMA_VERSION + 1) as i64)
            .unwrap();
        drop(conn);
        match Store::open_in(dir.path(), Role::Engine) {
            Err(StoreError::NewerSchema { found, supported }) => {
                assert_eq!(found, LATEST_SCHEMA_VERSION + 1);
                assert_eq!(supported, LATEST_SCHEMA_VERSION);
            }
            other => panic!("expected NewerSchema, got {other:?}"),
        }
    }
}
