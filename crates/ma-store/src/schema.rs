//! Table families and writer roles (contract-store-ownership), plus the artifact layout under a
//! root (contract-artifact-addressing).

use ma_core_types::{ArtifactKind, ChunkSeq, MeetingId, PathSegment, TrackId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableFamily {
    Session,
    Workflow,
    Export,
    Tombstone,
    Settings,
}

/// Which process opened the connection. Reads are unrestricted; writes carry the role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Engine,
    Interface,
}

impl Role {
    /// The closed writer assignment.
    pub fn families(self) -> &'static [TableFamily] {
        match self {
            Role::Engine => &[
                TableFamily::Session,
                TableFamily::Workflow,
                TableFamily::Export,
                TableFamily::Tombstone,
            ],
            Role::Interface => &[TableFamily::Settings],
        }
    }
    pub fn may_write(self, table: &str) -> bool {
        table_family(table).is_some_and(|f| self.families().contains(&f))
    }
}

/// Every product table and its family. `sqlite_*` internals are not product tables.
pub const TABLES: &[(&str, TableFamily)] = &[
    ("meeting", TableFamily::Session),
    ("session", TableFamily::Session),
    ("session_transition", TableFamily::Session),
    ("track", TableFamily::Session),
    ("chunk", TableFamily::Session),
    ("gap", TableFamily::Session),
    ("workflow_step", TableFamily::Workflow),
    ("work_item", TableFamily::Workflow),
    ("effect_ledger", TableFamily::Workflow),
    ("artifact", TableFamily::Workflow),
    ("generation", TableFamily::Workflow),
    ("edit_overlay", TableFamily::Workflow),
    ("export", TableFamily::Export),
    ("export_attempt", TableFamily::Export),
    ("egress_audit", TableFamily::Export),
    ("tombstone", TableFamily::Tombstone),
    ("settings", TableFamily::Settings),
    ("app_mode_override", TableFamily::Settings),
    ("roots", TableFamily::Settings),
];

pub fn table_family(table: &str) -> Option<TableFamily> {
    TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, f)| *f)
}

/// Tables that carry a `meeting_id` column and are cleared by the purge.
pub const MEETING_SCOPED_TABLES: &[&str] = &[
    "edit_overlay",
    "generation",
    "artifact",
    "effect_ledger",
    "work_item",
    "workflow_step",
    "export_attempt",
    "export",
    "egress_audit",
    "gap",
    "chunk",
    "track",
    "session_transition",
    "session",
    "meeting",
];

/// Relative artifact layout under a root. Every segment is a generated identifier or a fixed
/// literal; no user-supplied text can enter (contract-artifact-addressing).
pub struct ArtifactLayout;

impl ArtifactLayout {
    pub fn meeting_dir(meeting_id: MeetingId) -> Vec<PathSegment> {
        vec![
            PathSegment::new("meetings").expect("literal"),
            PathSegment::new(meeting_id.to_string()).expect("uuid"),
        ]
    }
    pub fn chunk(meeting_id: MeetingId, track_id: TrackId, seq: ChunkSeq) -> Vec<PathSegment> {
        let mut p = Self::meeting_dir(meeting_id);
        p.push(PathSegment::new(ArtifactKind::Chunks.dir_name()).expect("literal"));
        p.push(PathSegment::new(track_id.to_string()).expect("uuid"));
        p.push(PathSegment::new(format!("{seq}.wav")).expect("seq"));
        p
    }
    pub fn track_flac(meeting_id: MeetingId, track_id: TrackId) -> Vec<PathSegment> {
        let mut p = Self::meeting_dir(meeting_id);
        p.push(PathSegment::new(ArtifactKind::Consolidated.dir_name()).expect("literal"));
        p.push(PathSegment::new(format!("{track_id}.flac")).expect("uuid"));
        p
    }
    pub fn kind_dir(meeting_id: MeetingId, kind: ArtifactKind) -> Vec<PathSegment> {
        let mut p = Self::meeting_dir(meeting_id);
        p.push(PathSegment::new(kind.dir_name()).expect("literal"));
        p
    }
    pub fn join(segments: &[PathSegment]) -> String {
        segments
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("/")
    }
}
