//! Artifact rows and root resolution (contract-artifact-addressing).

use crate::repo::Store;
use crate::schema::ArtifactLayout;
use crate::{Result, StoreError};
use ma_core_types::{ArtifactId, ArtifactKind, MeetingId, PathSegment, RootId};
use std::path::PathBuf;

pub fn insert_artifact(
    store: &mut Store,
    artifact: ArtifactId,
    meeting: MeetingId,
    kind: ArtifactKind,
    root: RootId,
    segments: &[PathSegment],
    created_at: i64,
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO artifact (artifact_id, meeting_id, kind, root_id, relative_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (artifact.to_string(), meeting.to_string(), kind.dir_name(), root.to_string(), ArtifactLayout::join(segments), created_at),
    )?;
    Ok(())
}

/// The absolute path a root currently points at.
pub fn root_path(store: &Store, root: RootId) -> Result<PathBuf> {
    let path: String = store.conn().query_row(
        "SELECT absolute_path FROM roots WHERE root_id = ?1",
        [root.to_string()],
        |r| r.get(0),
    )?;
    Ok(PathBuf::from(path))
}

/// Resolve an artifact against its root's current location.
pub fn resolve(store: &Store, artifact: ArtifactId) -> Result<PathBuf> {
    let (root, relative): (String, String) = store.conn().query_row(
        "SELECT root_id, relative_path FROM artifact WHERE artifact_id = ?1",
        [artifact.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let root: RootId = root.parse()?;
    let mut path = root_path(store, root)?;
    for segment in relative.split('/') {
        path.push(segment);
    }
    Ok(path)
}

/// Create `meetings/<meeting_id>/` (with an empty `chunks/`) under the root. The path contains
/// only the identifier; no title or other text ever forms a segment.
pub fn ensure_meeting_dir(store: &Store, root: RootId, meeting: MeetingId) -> Result<PathBuf> {
    let root_dir = root_path(store, root)?;
    if !root_dir.is_dir() {
        return Err(StoreError::RootUnreachable(root_dir));
    }
    let mut dir = root_dir;
    for segment in ArtifactLayout::meeting_dir(meeting) {
        dir.push(segment.as_str());
    }
    std::fs::create_dir_all(dir.join(ArtifactKind::Chunks.dir_name()))?;
    Ok(dir)
}
