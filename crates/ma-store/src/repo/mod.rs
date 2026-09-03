//! Connection opening with role enforcement and the repository modules.

pub mod artifact;
pub mod export;
pub mod session;
pub mod settings;

use crate::migration::migrate;
use crate::schema::Role;
use crate::Result;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub const BUSY_TIMEOUT_MS: u32 = 5_000;
pub const DB_FILE_NAME: &str = "meeting-assistant.db";

/// One connection opened under a role. Reads are unrestricted; writes outside the role's
/// families are rejected by the SQLite authorizer before any statement runs.
pub struct Store {
    conn: Connection,
    role: Role,
    path: PathBuf,
}

impl Store {
    /// Open the pinned database (`%LOCALAPPDATA%\MeetingAssistant\db\`).
    pub fn open(role: Role) -> Result<Self> {
        let path = crate::pinned_db_path()?;
        Self::open_file(&path, role)
    }

    /// Test seam: open the database inside `dir` (the file name is fixed). Product code opens
    /// the pinned location through [`Store::open`].
    pub fn open_in(dir: &Path, role: Role) -> Result<Self> {
        Self::open_file(&dir.join(DB_FILE_NAME), role)
    }

    fn open_file(path: &Path, role: Role) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn)?;
        let store = Self {
            conn,
            role,
            path: path.to_path_buf(),
        };
        store.install_role_guard();
        Ok(store)
    }

    /// Writes outside the role's table families are denied by the SQLite authorizer, so a
    /// role-violating statement fails before it runs in every build, not only in debug.
    fn install_role_guard(&self) {
        let role = self.role;
        self.conn.authorizer(Some(move |ctx: AuthContext<'_>| {
            let target = match ctx.action {
                AuthAction::Insert { table_name }
                | AuthAction::Update { table_name, .. }
                | AuthAction::Delete { table_name } => Some(table_name),
                _ => None,
            };
            match target {
                Some(table) if !table.starts_with("sqlite_") && !role.may_write(table) => {
                    Authorization::Deny
                }
                _ => Authorization::Allow,
            }
        }));
    }

    pub fn role(&self) -> Role {
        self.role
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Run a read-modify-write in one `BEGIN IMMEDIATE` transaction.
    pub fn immediate<T>(
        &mut self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn pragma_str(&self, name: &str) -> Result<String> {
        Ok(self
            .conn
            .query_row(&format!("PRAGMA {name}"), [], |r| {
                r.get::<_, rusqlite::types::Value>(0)
            })
            .map(|v| match v {
                rusqlite::types::Value::Text(s) => s,
                rusqlite::types::Value::Integer(i) => i.to_string(),
                other => format!("{other:?}"),
            })?)
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("role", &self.role)
            .field("path", &self.path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ArtifactLayout, TableFamily, TABLES};
    use ma_core_types::id::TypedId;
    use ma_core_types::{
        ArtifactKind, ArtifactRef, ChunkSeq, MeetingId, RootId, SessionId, TrackId,
    };
    use proptest::prelude::*;

    fn engine(dir: &Path) -> Store {
        Store::open_in(dir, Role::Engine).unwrap()
    }
    fn interface(dir: &Path) -> Store {
        Store::open_in(dir, Role::Interface).unwrap()
    }

    #[test]
    fn wal_and_busy_timeout_configured() {
        let dir = tempfile::tempdir().unwrap();
        let store = engine(dir.path());
        assert_eq!(
            store.pragma_str("journal_mode").unwrap().to_lowercase(),
            "wal"
        );
        assert_eq!(
            store.pragma_str("synchronous").unwrap(),
            "1",
            "synchronous = NORMAL"
        );
        assert_eq!(store.pragma_str("foreign_keys").unwrap(), "1");
        assert_eq!(
            store.pragma_str("busy_timeout").unwrap(),
            BUSY_TIMEOUT_MS.to_string()
        );
    }

    #[test]
    fn database_path_is_pinned_to_local_appdata() {
        let dir = tempfile::tempdir().unwrap();
        // the pinned path derives only from LOCALAPPDATA; there is no setting for it
        std::env::set_var("LOCALAPPDATA", dir.path());
        let pinned = crate::pinned_db_path().unwrap();
        assert_eq!(
            pinned,
            dir.path()
                .join("MeetingAssistant")
                .join("db")
                .join(DB_FILE_NAME)
        );
        let store = Store::open(Role::Engine).unwrap();
        assert_eq!(store.path(), pinned.as_path());
        assert!(pinned.exists());
        // roots are the only relocatable location; the settings table has no database-path key
        let keys: Vec<String> = store
            .conn()
            .prepare("SELECT key FROM settings")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(keys
            .iter()
            .all(|k| !k.contains("db_path") && !k.contains("database")));
    }

    #[test]
    fn write_outside_role_family_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ui = interface(dir.path());
        let mut eng = engine(dir.path());
        let root = RootId::new();
        // the interface host owns settings and may insert a root; the engine may not
        ui.conn()
            .execute(
                "INSERT INTO roots (root_id, absolute_path, is_default) VALUES (?1, ?2, 1)",
                (root.to_string(), dir.path().to_string_lossy()),
            )
            .unwrap();
        let denied = eng.conn().execute(
            "INSERT INTO settings (key, value_json) VALUES ('x', '1')",
            [],
        );
        assert!(denied.is_err(), "engine writing settings must be rejected");
        // the engine owns the session family; the interface may not
        let meeting = MeetingId::new();
        eng.conn()
            .execute(
                "INSERT INTO meeting (meeting_id, created_at) VALUES (?1, 1)",
                [meeting.to_string()],
            )
            .unwrap();
        let denied = ui.conn().execute(
            "UPDATE meeting SET title = 'x' WHERE meeting_id = ?1",
            [meeting.to_string()],
        );
        assert!(
            denied.is_err(),
            "interface writing the session family must be rejected"
        );
        let denied = ui.conn().execute("INSERT INTO workflow_step (step_id, meeting_id, step_key, processor, version, config_hash, status) VALUES ('s', ?1, 'k', 'p', '1', 'h', 'queued')", [meeting.to_string()]);
        assert!(
            denied.is_err(),
            "interface writing workflow_step must be rejected"
        );
        // reads are unrestricted for both roles
        let n: i64 = ui
            .conn()
            .query_row("SELECT count(*) FROM meeting", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // every table has exactly one writer role
        for (table, family) in TABLES {
            let writers: Vec<Role> = [Role::Engine, Role::Interface]
                .into_iter()
                .filter(|r| r.may_write(table))
                .collect();
            assert_eq!(
                writers.len(),
                1,
                "{table} ({family:?}) must have exactly one writer"
            );
        }
        assert_eq!(Role::Interface.families(), &[TableFamily::Settings]);
        eng.immediate(|tx| {
            tx.execute(
                "UPDATE meeting SET title = 'ok' WHERE meeting_id = ?1",
                [meeting.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn no_absolute_artifact_path_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let mut ui = interface(dir.path());
        let mut eng = engine(dir.path());
        let root = RootId::new();
        settings::insert_root(&mut ui, root, dir.path()).unwrap();
        let meeting = MeetingId::new();
        let session = SessionId::new();
        let track = TrackId::new();
        session::insert_meeting(&mut eng, meeting, 1, Some("Weekly")).unwrap();
        session::insert_session(&mut eng, session, meeting, "recording", 1).unwrap();
        session::insert_track(&mut eng, track, session, 16_000).unwrap();
        session::insert_chunk(
            &mut eng,
            ma_core_types::ChunkId::new(),
            track,
            ChunkSeq(0),
            0,
            480_000,
            root,
            &ArtifactLayout::chunk(meeting, track, ChunkSeq(0)),
        )
        .unwrap();
        artifact::insert_artifact(
            &mut eng,
            ma_core_types::ArtifactId::new(),
            meeting,
            ArtifactKind::Transcript,
            root,
            &ArtifactLayout::kind_dir(meeting, ArtifactKind::Transcript),
            1,
        )
        .unwrap();
        for (table, column) in [("chunk", "relative_path"), ("artifact", "relative_path")] {
            let rows: Vec<String> = eng
                .conn()
                .prepare(&format!("SELECT {column} FROM {table}"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert!(!rows.is_empty());
            for value in rows {
                assert!(
                    !value.starts_with('/')
                        && !value.starts_with('\\')
                        && !value.contains(":\\")
                        && !value.starts_with("\\\\"),
                    "{table}.{column} holds an absolute or UNC path: {value}"
                );
            }
        }
        // the schema itself refuses an absolute, drive-letter or UNC relative_path
        for bad in [
            "/abs/x.wav",
            "C:\\x.wav",
            "\\\\server\\share\\x.wav",
            "../escape.wav",
        ] {
            let r = eng.conn().execute("INSERT INTO artifact (artifact_id, meeting_id, kind, root_id, relative_path, created_at) VALUES (?1, ?2, 'summary', ?3, ?4, 1)", (ma_core_types::ArtifactId::new().to_string(), meeting.to_string(), root.to_string(), bad));
            assert!(r.is_err(), "{bad} must be rejected by the CHECK constraint");
        }
    }

    #[test]
    fn root_relocation_preserves_references() {
        let dir = tempfile::tempdir().unwrap();
        let old_root = tempfile::tempdir().unwrap();
        let new_root = tempfile::tempdir().unwrap();
        let mut ui = interface(dir.path());
        let mut eng = engine(dir.path());
        let root = RootId::new();
        settings::insert_root(&mut ui, root, old_root.path()).unwrap();
        let meeting = MeetingId::new();
        session::insert_meeting(&mut eng, meeting, 1, None).unwrap();
        let artifact_id = ma_core_types::ArtifactId::new();
        let segments = ArtifactLayout::kind_dir(meeting, ArtifactKind::Summary);
        artifact::insert_artifact(
            &mut eng,
            artifact_id,
            meeting,
            ArtifactKind::Summary,
            root,
            &segments,
            1,
        )
        .unwrap();
        let before = artifact::resolve(&eng, artifact_id).unwrap();
        assert!(before.starts_with(old_root.path()));
        // relocating updates exactly one row
        let changed = settings::relocate_root(&mut ui, root, new_root.path()).unwrap();
        assert_eq!(changed, 1);
        let after = artifact::resolve(&eng, artifact_id).unwrap();
        assert!(after.starts_with(new_root.path()));
        assert_eq!(
            before.strip_prefix(old_root.path()).unwrap(),
            after.strip_prefix(new_root.path()).unwrap()
        );
        let stored: String = eng
            .conn()
            .query_row(
                "SELECT relative_path FROM artifact WHERE artifact_id = ?1",
                [artifact_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, ArtifactLayout::join(&segments));
    }

    #[test]
    fn identifier_identical_across_db_path_and_export() {
        let dir = tempfile::tempdir().unwrap();
        let root_dir = tempfile::tempdir().unwrap();
        let mut ui = interface(dir.path());
        let mut eng = engine(dir.path());
        let root = RootId::new();
        settings::insert_root(&mut ui, root, root_dir.path()).unwrap();
        let meeting = MeetingId::new();
        let session_id = SessionId::new();
        session::insert_meeting(&mut eng, meeting, 1, None).unwrap();
        session::insert_session(&mut eng, session_id, meeting, "recording", 1).unwrap();
        let dir_path = artifact::ensure_meeting_dir(&eng, root, meeting).unwrap();
        let row: String = eng
            .conn()
            .query_row(
                "SELECT meeting_id FROM session WHERE session_id = ?1",
                [session_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        let payload = export::ExportPayload::new(meeting, session_id);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(row, meeting.to_string());
        assert_eq!(
            dir_path.file_name().unwrap().to_string_lossy(),
            meeting.to_string(),
            "directory name is the id"
        );
        assert_eq!(
            json["meeting_id"],
            serde_json::Value::String(meeting.to_string())
        );
        assert_eq!(
            json["session_id"],
            serde_json::Value::String(session_id.to_string())
        );
        assert!(
            json.get("title").is_none(),
            "an export identifies the meeting by id, never by title"
        );
    }

    proptest! {
        #[test]
        fn hostile_titles_never_reach_the_filesystem(title in "[^\\p{Cc}]{1,300}") {
            let dir = tempfile::tempdir().unwrap();
            let root_dir = tempfile::tempdir().unwrap();
            let mut ui = interface(dir.path());
            let mut eng = engine(dir.path());
            let root = RootId::new();
            settings::insert_root(&mut ui, root, root_dir.path()).unwrap();
            let meeting = MeetingId::new();
            session::insert_meeting(&mut eng, meeting, 1, Some(&title)).unwrap();
            let path = artifact::ensure_meeting_dir(&eng, root, meeting).unwrap();
            let relative = path.strip_prefix(root_dir.path()).unwrap();
            for component in relative.components() {
                let c = component.as_os_str().to_string_lossy();
                prop_assert!(c == "meetings" || c == meeting.to_string(), "only fixed literals and identifiers: {c}");
            }
            prop_assert!(relative.to_string_lossy().len() < 64);
            let reference = ArtifactRef::new(root, ArtifactLayout::meeting_dir(meeting)).unwrap();
            prop_assert_eq!(reference.relative_path(), format!("meetings/{meeting}"), "the title never forms a path segment");
            let stored: Option<String> = eng.conn().query_row("SELECT title FROM meeting WHERE meeting_id = ?1", [meeting.to_string()], |r| r.get(0)).unwrap();
            prop_assert_eq!(stored.as_deref(), Some(title.as_str()), "the title is data in a row, not a path");
        }
    }
}
