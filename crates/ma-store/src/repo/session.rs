//! Session family repository (writer: engine).

use crate::repo::Store;
use crate::schema::ArtifactLayout;
use crate::Result;
use ma_core_types::{ChunkId, ChunkSeq, MeetingId, PathSegment, RootId, SessionId, TrackId};

pub fn insert_meeting(
    store: &mut Store,
    meeting: MeetingId,
    created_at: i64,
    title: Option<&str>,
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO meeting (meeting_id, created_at, title) VALUES (?1, ?2, ?3)",
        (meeting.to_string(), created_at, title),
    )?;
    Ok(())
}

pub fn insert_session(
    store: &mut Store,
    session: SessionId,
    meeting: MeetingId,
    state: &str,
    created_at: i64,
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO session (session_id, meeting_id, state, created_at) VALUES (?1, ?2, ?3, ?4)",
        (session.to_string(), meeting.to_string(), state, created_at),
    )?;
    Ok(())
}

pub fn insert_track(
    store: &mut Store,
    track: TrackId,
    session: SessionId,
    sample_rate: u32,
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO track (track_id, session_id, origin_wall_utc_ms, origin_monotonic_ns, sample_rate, channels, capture_mode, contamination_risk) VALUES (?1, ?2, 0, 0, ?3, 1, 'process_loopback', 'none')",
        (track.to_string(), session.to_string(), sample_rate),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_chunk(
    store: &mut Store,
    chunk: ChunkId,
    track: TrackId,
    seq: ChunkSeq,
    start_sample: u64,
    len_samples: u64,
    root: RootId,
    segments: &[PathSegment],
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO chunk (chunk_id, track_id, seq, start_sample, len_samples, root_id, relative_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (chunk.to_string(), track.to_string(), seq.0, start_sample as i64, len_samples as i64, root.to_string(), ArtifactLayout::join(segments)),
    )?;
    Ok(())
}
