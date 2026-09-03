//! Core types fixture.
//!
/// the corroboration requirement this adapter must meet
pub struct Corroboration;

/// Process name literal planted as a class B violation.
#[allow(dead_code)]
pub const PROCESS_IMAGE: &str = "Teams.exe";

pub fn weight() -> u32 {
    let graph_edge = 1u32;
    edge_weight(graph_edge)
}
fn edge_weight(graph_edge: u32) -> u32 { graph_edge + 1 }
#[allow(dead_code)]
pub const ENDED: &str = "meeting ended";
