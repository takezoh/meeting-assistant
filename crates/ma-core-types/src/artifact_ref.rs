//! Root-relative artifact addressing (contract-artifact-addressing): an artifact is a root
//! identifier plus a relative path composed only of generated identifiers and typed names, so
//! relocating the configurable root invalidates nothing.

use crate::error::CoreError;
use crate::id::RootId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// The typed directory names an artifact path may use besides generated identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Chunks,
    Consolidated,
    Transcript,
    Summary,
    Exports,
}

impl ArtifactKind {
    pub fn dir_name(self) -> &'static str {
        match self {
            ArtifactKind::Chunks => "chunks",
            ArtifactKind::Consolidated => "tracks",
            ArtifactKind::Transcript => "transcript",
            ArtifactKind::Summary => "summary",
            ArtifactKind::Exports => "exports",
        }
    }
}

/// One validated path segment: no separators, no `.`/`..`, ASCII letters, digits, `.`, `_`, `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PathSegment(String);

impl PathSegment {
    pub fn new(segment: impl Into<String>) -> Result<Self, CoreError> {
        let s = segment.into();
        let valid = !s.is_empty()
            && s.len() <= 128
            && s != "."
            && s != ".."
            && !s.starts_with('.')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if valid {
            Ok(PathSegment(s))
        } else {
            Err(CoreError::InvalidPathSegment(s))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PathSegment {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, CoreError> {
        PathSegment::new(value)
    }
}

impl From<PathSegment> for String {
    fn from(value: PathSegment) -> String {
        value.0
    }
}

/// A reference to a stored artifact: root identifier plus relative path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub root: RootId,
    pub segments: Vec<PathSegment>,
}

impl ArtifactRef {
    pub fn new(root: RootId, segments: Vec<PathSegment>) -> Result<Self, CoreError> {
        if segments.is_empty() {
            return Err(CoreError::InvalidPathSegment(String::new()));
        }
        Ok(Self { root, segments })
    }

    /// The root-relative path with `/` separators, identical in rows, paths and payloads.
    pub fn relative_path(&self) -> String {
        self.segments
            .iter()
            .map(PathSegment::as_str)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Resolve against the directory the root currently points at.
    pub fn resolve(&self, root_dir: &Path) -> PathBuf {
        let mut path = root_dir.to_path_buf();
        for segment in &self.segments {
            path.push(segment.as_str());
        }
        path
    }
}

impl fmt::Display for ArtifactRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.root, self.relative_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ChunkSeq, MeetingId, TypedId};

    #[test]
    fn relocating_the_root_keeps_every_reference_valid() {
        let root = RootId::new();
        let meeting = MeetingId::new();
        let reference = ArtifactRef::new(
            root,
            vec![
                PathSegment::new(meeting.as_path_segment()).unwrap(),
                PathSegment::new(ArtifactKind::Chunks.dir_name()).unwrap(),
                PathSegment::new(format!("{}.wav", ChunkSeq(4))).unwrap(),
            ],
        )
        .unwrap();
        let before = reference.resolve(Path::new("/old/root"));
        let after = reference.resolve(Path::new("/new/root"));
        assert_eq!(
            before.strip_prefix("/old/root").unwrap(),
            after.strip_prefix("/new/root").unwrap()
        );
        assert_eq!(
            reference.relative_path(),
            format!("{meeting}/chunks/000004.wav")
        );
        let json = serde_json::to_string(&reference).unwrap();
        assert!(json.contains(&meeting.to_string()));
        assert_eq!(
            serde_json::from_str::<ArtifactRef>(&json).unwrap(),
            reference
        );
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        for bad in ["..", ".", "", "a/b", "a\\b", ".hidden", "C:"] {
            assert!(PathSegment::new(bad).is_err(), "{bad:?} must be rejected");
        }
        assert!(serde_json::from_str::<PathSegment>("\"../etc\"").is_err());
    }
}
