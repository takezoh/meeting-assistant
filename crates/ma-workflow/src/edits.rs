//! Generated content is immutable and appended per run; user edits live in an overlay anchored to
//! the generation the user saw. Reading composes the two. Regeneration never touches the overlay;
//! an edit whose anchor is gone becomes `orphaned = true` and stays enumerable.

use ma_core_types::{ArtifactId, MeetingId, StepId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation {
    pub generation_id: uuid::Uuid,
    pub meeting_id: MeetingId,
    pub artifact_id: ArtifactId,
    pub step_id: StepId,
    pub produced_at_ms: u64,
    pub processor_id: String,
    pub model_id: String,
    pub adapter_version: String,
    /// The generated content keyed by anchor (speaker cluster id or segment id).
    pub content: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    SpeakerLabel,
    TranscriptText,
    SummaryText,
}

/// What an edit anchors to. Speaker edits anchor to the cluster, never to a segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    SpeakerCluster {
        cluster_id: String,
    },
    Segment {
        segment_id: String,
        text_hash: String,
    },
}

/// The anchor basis of the generation the user is looking at, captured at edit time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnchorBasis {
    pub generation_id: Option<uuid::Uuid>,
    pub speaker_clusters: Vec<String>,
    pub segments: BTreeMap<String, String>,
}

impl AnchorBasis {
    pub fn of(generation: &Generation, clusters: &[&str]) -> AnchorBasis {
        AnchorBasis {
            generation_id: Some(generation.generation_id),
            speaker_clusters: clusters.iter().map(|c| c.to_string()).collect(),
            segments: generation
                .content
                .iter()
                .filter(|(k, _)| k.starts_with("seg-"))
                .map(|(k, v)| (k.clone(), text_hash(v)))
                .collect(),
        }
    }
    fn contains(&self, anchor: &Anchor) -> bool {
        match anchor {
            Anchor::SpeakerCluster { cluster_id } => {
                self.speaker_clusters.iter().any(|c| c == cluster_id)
            }
            Anchor::Segment {
                segment_id,
                text_hash,
            } => self
                .segments
                .get(segment_id)
                .is_some_and(|h| h == text_hash),
        }
    }
}

pub fn text_hash(text: &str) -> String {
    use sha2::Digest;
    hex::encode(&sha2::Sha256::digest(text.as_bytes())[..8])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditOverlay {
    pub overlay_id: uuid::Uuid,
    pub meeting_id: MeetingId,
    pub artifact_id: ArtifactId,
    pub target_kind: TargetKind,
    pub anchor: Anchor,
    pub value: String,
    pub edited_at_ms: u64,
    pub orphaned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// An overlay with no anchor basis can never be re-applied, so it is refused rather than stored.
    NoAnchorBasis,
    AnchorNotInBasis,
}

/// Accept an edit only against a known anchor basis.
pub fn propose_edit(
    basis: Option<&AnchorBasis>,
    meeting_id: MeetingId,
    artifact_id: ArtifactId,
    target_kind: TargetKind,
    anchor: Anchor,
    value: &str,
    now_ms: u64,
) -> Result<EditOverlay, EditError> {
    let basis = basis.ok_or(EditError::NoAnchorBasis)?;
    if basis.generation_id.is_none() || !basis.contains(&anchor) {
        return Err(EditError::AnchorNotInBasis);
    }
    Ok(EditOverlay {
        overlay_id: uuid::Uuid::new_v5(
            &artifact_id_uuid(artifact_id),
            format!("{anchor:?}:{now_ms}").as_bytes(),
        ),
        meeting_id,
        artifact_id,
        target_kind,
        anchor,
        value: value.to_string(),
        edited_at_ms: now_ms,
        orphaned: false,
    })
}

fn artifact_id_uuid(id: ArtifactId) -> uuid::Uuid {
    use ma_core_types::id::TypedId;
    id.uuid()
}

/// After a new generation lands: mark overlays whose anchor is gone as orphaned. Never deletes.
pub fn reanchor(overlays: &mut [EditOverlay], new_basis: &AnchorBasis) {
    for overlay in overlays.iter_mut() {
        overlay.orphaned = !new_basis.contains(&overlay.anchor);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedView {
    /// Anchor → text as the user sees it: generation content with applied overlays on top.
    pub content: BTreeMap<String, String>,
    pub applied: usize,
    pub orphaned: Vec<uuid::Uuid>,
}

/// Latest generation plus overlay, composed at read time.
pub fn compose(generation: &Generation, overlays: &[EditOverlay]) -> ComposedView {
    let mut content = generation.content.clone();
    let mut applied = 0;
    let mut orphaned = Vec::new();
    for overlay in overlays {
        if overlay.orphaned {
            orphaned.push(overlay.overlay_id);
            continue;
        }
        let key = match &overlay.anchor {
            Anchor::SpeakerCluster { cluster_id } => cluster_id.clone(),
            Anchor::Segment { segment_id, .. } => segment_id.clone(),
        };
        content.insert(key, overlay.value.clone());
        applied += 1;
    }
    ComposedView {
        content,
        applied,
        orphaned,
    }
}
