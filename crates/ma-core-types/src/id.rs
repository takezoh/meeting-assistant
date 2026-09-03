//! UUIDv7 identifiers (contract-stable-identity).
//!
//! Every id is time-ordered, opaque to consumers and reproduced verbatim on all three surfaces:
//! database row, filesystem path segment and export payload. [`Display`], [`Serialize`] and
//! [`as_path_segment`](TypedId::as_path_segment) all yield the same lowercase hyphenated string.

use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Behaviour shared by every identifier newtype.
pub trait TypedId: Copy + Ord + fmt::Display + FromStr<Err = CoreError> {
    /// Mint a fresh time-ordered identifier.
    fn new() -> Self;
    /// The raw UUID.
    fn uuid(&self) -> Uuid;
    /// The identifier as a filesystem path segment; byte-identical to `to_string()`.
    fn as_path_segment(&self) -> String {
        self.to_string()
    }
}

macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl TypedId for $name {
            fn new() -> Self {
                Self(Uuid::now_v7())
            }
            fn uuid(&self) -> Uuid {
                self.0
            }
        }

        impl $name {
            /// Wrap an existing UUID (for example one read back from disk during recovery).
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.hyphenated())
            }
        }

        impl FromStr for $name {
            type Err = CoreError;
            fn from_str(s: &str) -> Result<Self, CoreError> {
                Uuid::parse_str(s).map(Self).map_err(|_| CoreError::InvalidId(s.to_string()))
            }
        }
    };
}

typed_id!(
    /// A meeting as the user perceives it; may span several sessions.
    MeetingId
);
typed_id!(
    /// One continuous recording; never changes across recovery.
    SessionId
);
typed_id!(
    /// One audio track segment (microphone, loopback) with its own origin.
    TrackId
);
typed_id!(
    /// One durable chunk file on a track.
    ChunkId
);
typed_id!(
    /// A stored artifact (transcript, summary, consolidated audio, export payload).
    ArtifactId
);
typed_id!(
    /// A workflow step identity derived from processor, version and configuration.
    StepId
);
typed_id!(
    /// One export attempt towards a destination.
    ExportId
);
typed_id!(
    /// One observed operating-system or extension signal.
    SignalId
);
typed_id!(
    /// One detector decision, citing the signals it used.
    DecisionId
);
typed_id!(
    /// A configurable artifact root; relocating the root keeps every reference valid.
    RootId
);

/// Dense per-track chunk sequence number, starting at zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChunkSeq(pub u32);

impl ChunkSeq {
    /// The next sequence number.
    pub fn next(self) -> Self {
        ChunkSeq(self.0 + 1)
    }
}

impl fmt::Display for ChunkSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:06}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuidv7_is_time_ordered() {
        let ids: Vec<SessionId> = (0..2_000).map(|_| SessionId::new()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(
            ids, sorted,
            "ids minted in sequence must sort in minting order"
        );
        assert!(ids.iter().all(|id| id.uuid().get_version_num() == 7));
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "ids must be unique without coordination"
        );
    }

    #[test]
    fn one_id_three_surfaces_one_string() {
        let id = MeetingId::new();
        let row = id.to_string();
        let path = id.as_path_segment();
        let payload = serde_json::to_string(&id).unwrap();
        assert_eq!(row, path);
        assert_eq!(payload, format!("\"{row}\""));
        let parsed: MeetingId = row.parse().unwrap();
        assert_eq!(parsed, id);
        assert_eq!(serde_json::from_str::<MeetingId>(&payload).unwrap(), id);
    }

    #[test]
    fn recovery_reuses_the_id_found_on_disk() {
        let on_disk = "018f4d2e-7b1a-7c3d-9a1e-0123456789ab";
        let recovered: SessionId = on_disk.parse().unwrap();
        assert_eq!(recovered.to_string(), on_disk);
        assert!("not-a-uuid".parse::<SessionId>().is_err());
    }
}
