//! Two-phase deletion (contract-retention-purge): `delete_meeting` hides a meeting in one
//! transaction; `purge` removes every byte and row and writes the tombstone that proves it.

use crate::repo::Store;
use crate::Result;
use ma_core_types::MeetingId;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What a purge run established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PurgeOutcome {
    /// Every listed meeting is purged and tombstoned.
    Determinate { purged: Vec<MeetingId> },
    /// Some meetings stay hidden and un-tombstoned: a purge is pending (interrupted, or blocked on
    /// an unresolved intended effect).
    Unknown { pending: Vec<MeetingId> },
    /// The artifact root is unreachable, so whether bytes remain cannot be decided.
    Inconclusive { root: String },
}

/// Why one meeting's purge did not complete this run; it stays pending and is retried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeIncomplete {
    pub meeting: MeetingId,
    /// The kind of obstacle only (a locked file, an intended effect) — never a path with content.
    pub reason: String,
}

/// Phase 1: hide the meeting and return the ids of in-flight steps to cancel.
pub fn delete_meeting(store: &mut Store, meeting: MeetingId, now_ms: i64) -> Result<Vec<String>> {
    store.immediate(|tx| {
        tx.execute("UPDATE meeting SET deleted_at = COALESCE(deleted_at, ?2) WHERE meeting_id = ?1", (meeting.to_string(), now_ms))?;
        let mut stmt = tx.prepare("SELECT step_id FROM workflow_step WHERE meeting_id = ?1 AND status IN ('queued', 'running')")?;
        let steps: Vec<String> = stmt.query_map([meeting.to_string()], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
        Ok(steps)
    })
}

/// Phase 2: idempotent, convergent purge driven from `deleted_at` rows.
pub fn purge(store: &mut Store, artifact_root: &Path, now_ms: i64) -> Result<PurgeOutcome> {
    purge_with_report(store, artifact_root, now_ms).map(|(outcome, _)| outcome)
}

/// The purge walk; also names why each pending meeting is still pending.
pub fn purge_with_report(
    store: &mut Store,
    artifact_root: &Path,
    now_ms: i64,
) -> Result<(PurgeOutcome, Vec<PurgeIncomplete>)> {
    // meetings hidden by phase 1 and not yet tombstoned, in deletion order
    let mut stmt = store.conn().prepare("SELECT m.meeting_id, m.created_at, m.deleted_at FROM meeting m LEFT JOIN tombstone t ON t.meeting_id = m.meeting_id WHERE m.deleted_at IS NOT NULL AND t.meeting_id IS NULL ORDER BY m.deleted_at, m.meeting_id")?;
    let rows: Vec<(String, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    if rows.is_empty() {
        return Ok((PurgeOutcome::Determinate { purged: vec![] }, Vec::new()));
    }
    if !artifact_root.is_dir() {
        return Ok((
            PurgeOutcome::Inconclusive {
                root: artifact_root.display().to_string(),
            },
            Vec::new(),
        ));
    }
    let mut purged = Vec::new();
    let mut pending = Vec::new();
    let mut incomplete: Vec<PurgeIncomplete> = Vec::new();
    for (id, created_at, deleted_at) in rows {
        let meeting: MeetingId = id.parse()?;
        // never race an effect that is about to create something
        let intended: i64 = store.conn().query_row(
            "SELECT count(*) FROM effect_ledger WHERE meeting_id = ?1 AND state = 'intended'",
            [&id],
            |r| r.get(0),
        )?;
        if intended > 0 {
            incomplete.push(PurgeIncomplete {
                meeting,
                reason: "blocked_on_intended_effect".into(),
            });
            pending.push(meeting);
            continue;
        }
        // remote resources this application created survive in the tombstone; they are never deleted
        let mut refs: Vec<String> = Vec::new();
        let mut stmt = store.conn().prepare("SELECT remote_id FROM export WHERE meeting_id = ?1 AND remote_id IS NOT NULL UNION SELECT remote_ref FROM egress_audit WHERE meeting_id = ?1 AND remote_ref IS NOT NULL UNION SELECT remote_ref FROM effect_ledger WHERE meeting_id = ?1 AND remote_ref IS NOT NULL")?;
        for r in stmt.query_map([&id], |r| r.get::<_, String>(0))? {
            refs.push(r?);
        }
        drop(stmt);
        refs.sort();
        refs.dedup();
        // bytes first: a tombstone is written only after a completed walk of a reachable root
        let dir = artifact_root.join("meetings").join(&id);
        if dir.exists() {
            // one undeletable file must not abort the run: this meeting stays pending, the rest proceed
            if let Err(err) = std::fs::remove_dir_all(&dir) {
                incomplete.push(PurgeIncomplete {
                    meeting,
                    reason: format!("file_not_removable:{:?}", err.kind()),
                });
                pending.push(meeting);
                continue;
            }
        }
        if dir.exists() {
            incomplete.push(PurgeIncomplete {
                meeting,
                reason: "directory_still_present".into(),
            });
            pending.push(meeting);
            continue;
        }
        let refs_json = serde_json::to_string(&refs).expect("refs serialize");
        store.immediate(|tx| {
            tx.execute("DELETE FROM edit_overlay WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM generation WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM artifact WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM effect_ledger WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM work_item WHERE step_id IN (SELECT step_id FROM workflow_step WHERE meeting_id = ?1)", [&id])?;
            tx.execute("DELETE FROM workflow_step WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM export_attempt WHERE export_id IN (SELECT export_id FROM export WHERE meeting_id = ?1)", [&id])?;
            tx.execute("DELETE FROM export WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM egress_audit WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM gap WHERE track_id IN (SELECT track_id FROM track WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1))", [&id])?;
            tx.execute("DELETE FROM chunk WHERE track_id IN (SELECT track_id FROM track WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1))", [&id])?;
            tx.execute("DELETE FROM track WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1)", [&id])?;
            tx.execute("DELETE FROM session_transition WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1)", [&id])?;
            tx.execute("DELETE FROM session WHERE meeting_id = ?1", [&id])?;
            tx.execute("DELETE FROM meeting WHERE meeting_id = ?1", [&id])?;
            tx.execute("INSERT INTO tombstone (meeting_id, created_at, deleted_at, purged_at, remote_resource_refs) VALUES (?1, ?2, ?3, ?4, ?5)", (&id, created_at, deleted_at, now_ms, &refs_json))?;
            Ok(())
        })?;
        purged.push(meeting);
    }
    if pending.is_empty() {
        Ok((PurgeOutcome::Determinate { purged }, incomplete))
    } else {
        Ok((PurgeOutcome::Unknown { pending }, incomplete))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{artifact, export, session, settings, Store};
    use crate::schema::{ArtifactLayout, Role, MEETING_SCOPED_TABLES};
    use ma_core_types::id::TypedId;
    use ma_core_types::{
        ArtifactId, ArtifactKind, ChunkId, ChunkSeq, ExportId, MeetingId, RootId, SessionId,
        TrackId,
    };

    struct Fixture {
        _db: tempfile::TempDir,
        root_dir: tempfile::TempDir,
        engine: Store,
        meeting: MeetingId,
    }

    fn fixture() -> Fixture {
        let db = tempfile::tempdir().unwrap();
        let root_dir = tempfile::tempdir().unwrap();
        let mut ui = Store::open_in(db.path(), Role::Interface).unwrap();
        let mut engine = Store::open_in(db.path(), Role::Engine).unwrap();
        let root = RootId::new();
        settings::insert_root(&mut ui, root, root_dir.path()).unwrap();
        let meeting = MeetingId::new();
        let session_id = SessionId::new();
        let track = TrackId::new();
        session::insert_meeting(&mut engine, meeting, 1, Some("Quarterly")).unwrap();
        session::insert_session(&mut engine, session_id, meeting, "completed", 1).unwrap();
        session::insert_track(&mut engine, track, session_id, 16_000).unwrap();
        let dir = artifact::ensure_meeting_dir(&engine, root, meeting).unwrap();
        for seq in 0..3u32 {
            let segments = ArtifactLayout::chunk(meeting, track, ChunkSeq(seq));
            let path = root_dir.path().join(ArtifactLayout::join(&segments));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"RIFF").unwrap();
            session::insert_chunk(
                &mut engine,
                ChunkId::new(),
                track,
                ChunkSeq(seq),
                seq as u64 * 480_000,
                480_000,
                root,
                &segments,
            )
            .unwrap();
        }
        std::fs::create_dir_all(dir.join("transcript")).unwrap();
        std::fs::write(dir.join("transcript").join("segments.json"), b"[]").unwrap();
        artifact::insert_artifact(
            &mut engine,
            ArtifactId::new(),
            meeting,
            ArtifactKind::Transcript,
            root,
            &ArtifactLayout::kind_dir(meeting, ArtifactKind::Transcript),
            1,
        )
        .unwrap();
        export::insert_export(
            &mut engine,
            ExportId::new(),
            meeting,
            "drive",
            Some("drive-file-123"),
        )
        .unwrap();
        export::record_egress(
            &mut engine,
            Some(meeting),
            "drive",
            "www.googleapis.com",
            "export",
            None,
            4096,
            Some("drive-file-123"),
            "ok",
            2,
        )
        .unwrap();
        // every remaining meeting-scoped table gets a row, so dropping any DELETE in purge() is caught
        let step_id = ma_core_types::StepId::new().to_string();
        let artifact_id: String = engine
            .conn()
            .query_row(
                "SELECT artifact_id FROM artifact WHERE meeting_id = ?1",
                [meeting.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        let export_id: String = engine
            .conn()
            .query_row(
                "SELECT export_id FROM export WHERE meeting_id = ?1",
                [meeting.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        for sql in [
            format!("INSERT INTO gap (track_id, from_sample, to_sample, reason) VALUES ('{track}', 1440000, 1500000, 'chunk_lost')"),
            format!("INSERT INTO session_transition (session_id, from_state, to_state, event, cause_kind, cause_refs, at_unbiased_ms) VALUES ('{session_id}', 'idle', 'candidate', 'detector_start', 'signal', '[]', 1)"),
            format!("INSERT INTO workflow_step (step_id, meeting_id, step_key, processor, version, config_hash, status, result_ref) VALUES ('{step_id}', '{meeting}', 'k-{step_id}', 'example-stt', '1', 'cfg', 'succeeded', NULL)"),
            format!("INSERT INTO work_item (work_item_id, step_id, ordinal, status) VALUES ('wi-{step_id}', '{step_id}', 0, 'done')"),
            format!("INSERT INTO effect_ledger (effect_id, meeting_id, step_id, kind, idempotency_key, state, remote_ref, at_ms) VALUES ('ef-{step_id}', '{meeting}', '{step_id}', 'artifact', 'k', 'committed', NULL, 2)"),
            format!("INSERT INTO generation (generation_id, meeting_id, artifact_id, step_id, processor_id, model_id, adapter_version, created_at) VALUES ('gen-{step_id}', '{meeting}', '{artifact_id}', '{step_id}', 'example-stt', 'model-a', '1', 2)"),
            format!("INSERT INTO edit_overlay (overlay_id, meeting_id, artifact_id, target_kind, anchor, value_json, edited_at, orphaned) VALUES ('ov-{step_id}', '{meeting}', '{artifact_id}', 'speaker_label', 'cluster-1', '\"Alice\"', 3, 0)"),
            format!("INSERT INTO export_attempt (export_id, started_at, outcome) VALUES ('{export_id}', 4, 'ok')"),
        ] {
            engine.conn().execute(&sql, []).unwrap();
        }
        Fixture {
            _db: db,
            root_dir,
            engine,
            meeting,
        }
    }

    /// Every meeting-scoped table, following foreign keys for the ones without a meeting_id column,
    /// so deleting any single DELETE statement from purge() makes this non-empty.
    fn meeting_id_appears_outside_tombstone(store: &Store, meeting: MeetingId) -> Vec<String> {
        let mut hits = Vec::new();
        for table in MEETING_SCOPED_TABLES {
            let sql = match *table {
                "session_transition" => "SELECT count(*) FROM session_transition WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1)".to_string(),
                "track" => "SELECT count(*) FROM track WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1)".to_string(),
                "chunk" | "gap" => format!("SELECT count(*) FROM {table} WHERE track_id IN (SELECT track_id FROM track WHERE session_id IN (SELECT session_id FROM session WHERE meeting_id = ?1))"),
                "work_item" => "SELECT count(*) FROM work_item WHERE step_id IN (SELECT step_id FROM workflow_step WHERE meeting_id = ?1)".to_string(),
                "export_attempt" => "SELECT count(*) FROM export_attempt WHERE export_id IN (SELECT export_id FROM export WHERE meeting_id = ?1)".to_string(),
                _ => format!("SELECT count(*) FROM {table} WHERE meeting_id = ?1"),
            };
            let n: i64 = store
                .conn()
                .query_row(&sql, [meeting.to_string()], |r| r.get(0))
                .unwrap();
            if n > 0 {
                hits.push(table.to_string());
            }
        }
        // orphan rows whose parent was deleted first would also escape a meeting_id query
        for (table, parent, key) in [
            ("session_transition", "session", "session_id"),
            ("track", "session", "session_id"),
            ("chunk", "track", "track_id"),
            ("gap", "track", "track_id"),
            ("work_item", "workflow_step", "step_id"),
            ("export_attempt", "export", "export_id"),
        ] {
            let orphans: i64 = store.conn().query_row(&format!("SELECT count(*) FROM {table} WHERE {key} NOT IN (SELECT {key} FROM {parent})"), [], |r| r.get(0)).unwrap();
            if orphans > 0 {
                hits.push(format!("{table} (orphaned rows)"));
            }
        }
        hits
    }

    fn paths_containing(root: &Path, needle: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.to_string_lossy().contains(needle) {
                    out.push(p.to_string_lossy().into_owned());
                }
                if p.is_dir() {
                    stack.push(p);
                }
            }
        }
        out
    }

    #[test]
    fn purge_leaves_only_tombstone() {
        let mut f = fixture();
        let id = f.meeting.to_string();
        let covered = meeting_id_appears_outside_tombstone(&f.engine, f.meeting);
        let uncovered: Vec<&&str> = MEETING_SCOPED_TABLES
            .iter()
            .filter(|t| !covered.iter().any(|c| c == **t))
            .collect();
        assert!(uncovered.is_empty(), "the fixture must populate every meeting-scoped table so a dropped DELETE is caught: {uncovered:?}");
        assert!(!paths_containing(f.root_dir.path(), &id).is_empty());
        // phase 1: hidden in one transaction
        delete_meeting(&mut f.engine, f.meeting, 10).unwrap();
        let hidden: Option<i64> = f
            .engine
            .conn()
            .query_row(
                "SELECT deleted_at FROM meeting WHERE meeting_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, Some(10));
        let visible: i64 = f
            .engine
            .conn()
            .query_row(
                "SELECT count(*) FROM meeting WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(visible, 0, "invisible to every view after phase 1");
        assert!(
            !paths_containing(f.root_dir.path(), &id).is_empty(),
            "bytes may still exist before the tombstone"
        );
        // phase 2
        let outcome = purge(&mut f.engine, f.root_dir.path(), 20).unwrap();
        assert_eq!(
            outcome,
            PurgeOutcome::Determinate {
                purged: vec![f.meeting]
            }
        );
        assert!(
            paths_containing(f.root_dir.path(), &id).is_empty(),
            "no path under the root contains the meeting id"
        );
        assert!(
            meeting_id_appears_outside_tombstone(&f.engine, f.meeting).is_empty(),
            "no row outside tombstone references the meeting"
        );
        let (created_at, deleted_at, refs): (i64, i64, String) = f.engine.conn().query_row("SELECT created_at, deleted_at, remote_resource_refs FROM tombstone WHERE meeting_id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((created_at, deleted_at), (1, 10));
        assert!(
            refs.contains("drive-file-123"),
            "the tombstone lists remote resources this application created: {refs}"
        );
        assert!(
            !refs.contains("Quarterly"),
            "nothing in the tombstone identifies content"
        );
        let cols: Vec<String> = f
            .engine
            .conn()
            .prepare("SELECT name FROM pragma_table_info('tombstone')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            cols,
            vec![
                "meeting_id",
                "created_at",
                "deleted_at",
                "purged_at",
                "remote_resource_refs"
            ]
        );
    }

    #[test]
    fn purge_rerun_is_idempotent() {
        let mut f = fixture();
        delete_meeting(&mut f.engine, f.meeting, 10).unwrap();
        // a purge interrupted mid-walk: only part of the directory is gone, no tombstone yet
        let dir = f
            .root_dir
            .path()
            .join(ArtifactLayout::join(&ArtifactLayout::meeting_dir(
                f.meeting,
            )));
        std::fs::remove_dir_all(dir.join("transcript")).unwrap();
        let tombstones = |s: &Store| -> i64 {
            s.conn()
                .query_row("SELECT count(*) FROM tombstone", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(tombstones(&f.engine), 0);
        let first = purge(&mut f.engine, f.root_dir.path(), 20).unwrap();
        assert_eq!(
            first,
            PurgeOutcome::Determinate {
                purged: vec![f.meeting]
            },
            "resumes from deleted_at alone and converges"
        );
        assert_eq!(tombstones(&f.engine), 1);
        let second = purge(&mut f.engine, f.root_dir.path(), 30).unwrap();
        assert_eq!(
            second,
            PurgeOutcome::Determinate { purged: vec![] },
            "a second run is a no-op returning success"
        );
        assert_eq!(tombstones(&f.engine), 1);
        // an unresolved intended effect blocks the purge of that meeting
        let mut g = fixture();
        g.engine.conn().execute("INSERT INTO effect_ledger (effect_id, meeting_id, kind, idempotency_key, state, at_ms) VALUES ('e1', ?1, 'export.create', 'k1', 'intended', 5)", [g.meeting.to_string()]).unwrap();
        delete_meeting(&mut g.engine, g.meeting, 10).unwrap();
        let blocked = purge(&mut g.engine, g.root_dir.path(), 20).unwrap();
        assert_eq!(
            blocked,
            PurgeOutcome::Unknown {
                pending: vec![g.meeting]
            }
        );
        assert_eq!(
            tombstones(&g.engine),
            0,
            "no tombstone while an intended effect is unresolved"
        );
        assert!(
            !paths_containing(g.root_dir.path(), &g.meeting.to_string()).is_empty(),
            "the walk does not proceed past an unresolved effect"
        );
        // an unreachable root is inconclusive: hidden, not tombstoned, retried later
        let mut h = fixture();
        delete_meeting(&mut h.engine, h.meeting, 10).unwrap();
        let missing = h.root_dir.path().join("unplugged");
        assert!(matches!(
            purge(&mut h.engine, &missing, 20).unwrap(),
            PurgeOutcome::Inconclusive { .. }
        ));
        assert_eq!(tombstones(&h.engine), 0);
    }

    #[test]
    fn undeletable_meeting_stays_pending_and_does_not_abort_the_run() {
        let mut f = fixture();
        // a second hidden meeting whose bytes cannot be removed: a regular file where the meeting
        // directory should be makes remove_dir_all fail deterministically on every platform
        let stuck = MeetingId::new();
        session::insert_meeting(&mut f.engine, stuck, 1, None).unwrap();
        let stuck_dir = f.root_dir.path().join("meetings").join(stuck.to_string());
        std::fs::write(&stuck_dir, b"not a directory").unwrap();
        delete_meeting(&mut f.engine, stuck, 5).unwrap();
        delete_meeting(&mut f.engine, f.meeting, 10).unwrap();
        let (outcome, report) = purge_with_report(&mut f.engine, f.root_dir.path(), 20).unwrap();
        assert_eq!(
            outcome,
            PurgeOutcome::Unknown {
                pending: vec![stuck]
            },
            "the other meeting was purged in the same run"
        );
        assert_eq!(
            report,
            vec![PurgeIncomplete {
                meeting: stuck,
                reason: "file_not_removable:NotADirectory".into()
            }]
        );
        let tomb: i64 = f
            .engine
            .conn()
            .query_row(
                "SELECT count(*) FROM tombstone WHERE meeting_id = ?1",
                [f.meeting.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tomb, 1, "the deletable meeting reached its tombstone");
        let stuck_tomb: i64 = f
            .engine
            .conn()
            .query_row(
                "SELECT count(*) FROM tombstone WHERE meeting_id = ?1",
                [stuck.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stuck_tomb, 0, "no tombstone while bytes may remain");
        // the obstacle goes away: the next run converges
        std::fs::remove_file(&stuck_dir).unwrap();
        let (outcome, report) = purge_with_report(&mut f.engine, f.root_dir.path(), 30).unwrap();
        assert_eq!(
            outcome,
            PurgeOutcome::Determinate {
                purged: vec![stuck]
            }
        );
        assert!(report.is_empty());
    }
}
