//! `decide`: fold a signal timeline into evidence-citing decisions.

use crate::adapter::{AdapterTable, MatchKind};
use crate::decision::{Decision, DetectorOutput, Diagnostic};
use crate::outcome::{partition, Outcome, Phase, SuppressionReason};
use ma_core_types::SignalId;
use ma_signal::{Authority, SignalKind, SignalTimeline};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DetectorConfig {
    /// Whether an unmatched subject with microphone use is surfaced as a generic candidate.
    pub generic_detection_enabled: bool,
}

/// Evidence gathered for one adapter's candidate meeting.
#[derive(Debug, Clone, Default)]
struct Candidate {
    subject_key: Option<String>,
    microphone: Option<SignalId>,
    microphone_subject_key: Option<String>,
    tab: Option<SignalId>,
    tab_subject_key: Option<String>,
    /// `process_tree_root_pid` carried by the microphone signal, for the browser join.
    microphone_tree_root: Option<u32>,
    /// `process_tree_root_pid` carried by the tab signal, for the browser join.
    tab_tree_root: Option<u32>,
    resync: bool,
}

/// The process-tree join for adapters that require both tab and microphone evidence
/// (contract-meet-corroboration-required): corroboration holds only when both sides carry a tree
/// root and the roots are equal.
fn tree_join(candidate: &Candidate) -> Result<(), &'static str> {
    match (candidate.tab_tree_root, candidate.microphone_tree_root) {
        (Some(tab), Some(mic)) if tab == mic => Ok(()),
        (Some(_), Some(_)) => Err("process-tree-mismatch"),
        _ => Err("process-tree-root-absent"),
    }
}

fn same_tree(candidate_tree: Option<u32>, ending_tree: Option<u32>) -> bool {
    match (candidate_tree, ending_tree) {
        (Some(a), Some(b)) => a == b,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// Clear only the evidence slot owned by the ending signal. Process teardown can invalidate both
/// slots on its tree; audio-session/microphone teardown invalidates only microphone evidence.
/// Returns true when the candidate has no corroborating evidence left.
fn clear_candidate_evidence(
    candidate: &mut Candidate,
    kind: SignalKind,
    ending_tree: Option<u32>,
) -> bool {
    if kind == SignalKind::ProcessStopped {
        if same_tree(candidate.tab_tree_root, ending_tree) {
            candidate.tab = None;
            candidate.tab_tree_root = None;
            candidate.tab_subject_key = None;
        }
        if same_tree(candidate.microphone_tree_root, ending_tree) {
            candidate.microphone = None;
            candidate.microphone_tree_root = None;
            candidate.microphone_subject_key = None;
            candidate.resync = false;
        }
    } else if matches!(
        kind,
        SignalKind::MicCaptureStopped | SignalKind::AudioSessionDestroyed
    ) && same_tree(candidate.microphone_tree_root, ending_tree)
    {
        candidate.microphone = None;
        candidate.microphone_tree_root = None;
        candidate.microphone_subject_key = None;
        candidate.resync = false;
    }
    candidate.subject_key = candidate
        .tab_subject_key
        .clone()
        .or_else(|| candidate.microphone_subject_key.clone());
    candidate.tab.is_none() && candidate.microphone.is_none()
}

#[derive(Debug, Clone)]
struct Active {
    subject_key: Option<String>,
    started_at: u64,
    weight: u8,
    evidence: Vec<SignalId>,
    /// The browser process tree the meeting was joined on (browser-class adapters only), so an
    /// end signal from another tree cannot end it: the end path mirrors the start join.
    tree_root: Option<u32>,
}

pub fn decide(
    timeline: &SignalTimeline,
    config: &DetectorConfig,
    table: &mut AdapterTable,
) -> DetectorOutput {
    let version = table.version();
    let mut output = DetectorOutput {
        adapter_table_version: version,
        ..DetectorOutput::default()
    };
    let mut candidates: BTreeMap<String, Candidate> = BTreeMap::new();
    let mut active: BTreeMap<String, Active> = BTreeMap::new();

    for signal in timeline.signals() {
        let now = signal.observed_at.monotonic_ns;
        let (matched, disabled) = table.matches(&signal.subject);
        for adapter_id in disabled {
            output.diagnostics.push(Diagnostic { adapter_id: adapter_id.clone(), message: format!("adapter panicked while matching signal {} and is disabled for the remainder of the process", signal.signal_id) });
        }
        if matched.is_empty() {
            if signal.kind == SignalKind::MicCaptureStarted && signal.authority == Authority::Os {
                let outcome = partition(false, false, None);
                let rule = if config.generic_detection_enabled {
                    "generic-candidate"
                } else {
                    "no-adapter"
                };
                output.decisions.push(Decision::derive(
                    version,
                    outcome,
                    None,
                    Some(signal.subject.key()),
                    rule,
                    vec![signal.signal_id],
                    now,
                ));
            }
            continue;
        }
        for (adapter_id, kind) in matched {
            let adapter = table
                .adapter(&adapter_id)
                .expect("matched adapters are registered");
            let (weight, needs) = (adapter.evidence_weight(), adapter.corroboration());
            let subject_key = signal.subject.key();
            let is_end = matches!(
                signal.kind,
                SignalKind::MicCaptureStopped
                    | SignalKind::ProcessStopped
                    | SignalKind::AudioSessionDestroyed
            );
            if is_end {
                let ending_tree = signal.payload.process_tree_root_pid;
                let remove_candidate = candidates.get_mut(&adapter_id).is_some_and(|candidate| {
                    clear_candidate_evidence(candidate, signal.kind, ending_tree)
                });
                if remove_candidate {
                    candidates.remove(&adapter_id);
                }
                if let Some(meeting) = active.get(&adapter_id) {
                    // For a browser-class meeting joined on a process tree, only that tree's
                    // microphone stop ends it; another tree's stop is as foreign as its start.
                    let same_tree = match (meeting.tree_root, signal.payload.process_tree_root_pid)
                    {
                        (Some(a), Some(b)) => a == b,
                        (Some(_), None) => false,
                        (None, _) => true,
                    };
                    if (meeting.subject_key.as_deref() == Some(subject_key.as_str())
                        || kind == MatchKind::Process)
                        && same_tree
                    {
                        let mut evidence = meeting.evidence.clone();
                        evidence.push(signal.signal_id);
                        output.decisions.push(Decision::derive(
                            version,
                            Outcome::Determinate { phase: Phase::End },
                            Some(adapter_id.clone()),
                            meeting.subject_key.clone(),
                            "end",
                            evidence,
                            now,
                        ));
                        active.remove(&adapter_id);
                    }
                }
                continue;
            }
            if active.contains_key(&adapter_id) {
                // continuing evidence for an active meeting
                let meeting = active.get_mut(&adapter_id).expect("checked");
                meeting.evidence.push(signal.signal_id);
                output.decisions.push(Decision::derive(
                    version,
                    Outcome::Determinate {
                        phase: Phase::Continue,
                    },
                    Some(adapter_id.clone()),
                    meeting.subject_key.clone(),
                    "continue",
                    vec![signal.signal_id],
                    now,
                ));
                continue;
            }
            let candidate = candidates.entry(adapter_id.clone()).or_default();
            match (signal.kind, signal.authority, kind) {
                (SignalKind::MicCaptureStarted, Authority::Os, MatchKind::Process) => {
                    candidate.microphone = Some(signal.signal_id);
                    candidate.microphone_subject_key = Some(subject_key.clone());
                    candidate.microphone_tree_root = signal.payload.process_tree_root_pid;
                    candidate.subject_key.get_or_insert(subject_key.clone());
                    if signal.payload.restart_resync {
                        candidate.resync = true;
                    }
                }
                // only a meeting-present report corroborates; audibility alone is a landing page
                (SignalKind::TabMeetingPresent, Authority::Extension, MatchKind::Tab) => {
                    candidate.tab = Some(signal.signal_id);
                    candidate.tab_subject_key = Some(subject_key.clone());
                    candidate.tab_tree_root = signal.payload.process_tree_root_pid;
                    if candidate.subject_key.is_none() {
                        candidate.subject_key = Some(subject_key.clone());
                    }
                }
                _ => {}
            }
            let met = (!needs.microphone || candidate.microphone.is_some())
                && (!needs.tab || candidate.tab.is_some());
            // Both sides present for a browser-class adapter: they must come from the same
            // browser process tree, or the tab is not corroborated by this microphone use.
            if met && needs.tab && needs.microphone {
                if let Err(rule) = tree_join(candidate) {
                    let cited: Vec<SignalId> = [candidate.tab, candidate.microphone]
                        .into_iter()
                        .flatten()
                        .collect();
                    output.decisions.push(Decision::derive(
                        version,
                        Outcome::Inconclusive,
                        Some(adapter_id.clone()),
                        candidate.subject_key.clone(),
                        rule,
                        cited,
                        now,
                    ));
                    continue;
                }
            }
            let competing = active
                .iter()
                .find(|(id, _)| *id != &adapter_id)
                .map(|(id, m)| (id.clone(), m.weight, m.started_at));
            let outcome = partition(true, met, competing.as_ref().map(|(id, _, _)| id.as_str()));
            let cited: Vec<SignalId> = [candidate.tab, candidate.microphone]
                .into_iter()
                .flatten()
                .collect();
            let cited = if cited.is_empty() {
                vec![signal.signal_id]
            } else {
                cited
            };
            match outcome {
                Outcome::Inconclusive => {
                    let rule =
                        if needs.tab && candidate.tab.is_some() && candidate.microphone.is_none() {
                            "extension-alone"
                        } else {
                            "corroboration-missing"
                        };
                    output.decisions.push(Decision::derive(
                        version,
                        Outcome::Inconclusive,
                        Some(adapter_id.clone()),
                        candidate.subject_key.clone(),
                        rule,
                        cited,
                        now,
                    ));
                }
                Outcome::Determinate { .. } if candidate.resync => {
                    output.decisions.push(Decision::derive(
                        version,
                        Outcome::Inconclusive,
                        Some(adapter_id.clone()),
                        candidate.subject_key.clone(),
                        "resync-no-autostart",
                        cited,
                        now,
                    ));
                }
                Outcome::Determinate { .. } => {
                    let started = Active {
                        subject_key: candidate.subject_key.clone(),
                        started_at: now,
                        weight,
                        evidence: cited.clone(),
                        tree_root: if needs.tab && needs.microphone {
                            candidate.tab_tree_root
                        } else {
                            None
                        },
                    };
                    output.decisions.push(Decision::derive(
                        version,
                        Outcome::Determinate {
                            phase: Phase::Start,
                        },
                        Some(adapter_id.clone()),
                        candidate.subject_key.clone(),
                        "adapter-match",
                        cited,
                        now,
                    ));
                    active.insert(adapter_id.clone(), started);
                    candidates.remove(&adapter_id);
                }
                Outcome::Conflicting { .. } => {
                    let (active_id, active_weight, _) =
                        competing.expect("conflicting implies a competing active meeting");
                    if weight > active_weight {
                        // the newcomer takes precedence; the previously active meeting is recorded as suppressed
                        let previous = active.remove(&active_id).expect("present");
                        output.decisions.push(Decision::derive(
                            version,
                            Outcome::Conflicting {
                                suppressed: SuppressionReason::LowerPrecedence {
                                    active_adapter_id: adapter_id.clone(),
                                },
                            },
                            Some(active_id),
                            previous.subject_key,
                            "precedence",
                            previous.evidence,
                            now,
                        ));
                        let started = Active {
                            subject_key: candidate.subject_key.clone(),
                            started_at: now,
                            weight,
                            evidence: cited.clone(),
                            tree_root: if needs.tab && needs.microphone {
                                candidate.tab_tree_root
                            } else {
                                None
                            },
                        };
                        output.decisions.push(Decision::derive(
                            version,
                            Outcome::Determinate {
                                phase: Phase::Start,
                            },
                            Some(adapter_id.clone()),
                            candidate.subject_key.clone(),
                            "adapter-match",
                            cited,
                            now,
                        ));
                        active.insert(adapter_id.clone(), started);
                        candidates.remove(&adapter_id);
                    } else {
                        output.decisions.push(Decision::derive(
                            version,
                            Outcome::Conflicting {
                                suppressed: SuppressionReason::LowerPrecedence {
                                    active_adapter_id: active_id,
                                },
                            },
                            Some(adapter_id.clone()),
                            candidate.subject_key.clone(),
                            "precedence",
                            cited,
                            now,
                        ));
                    }
                }
                Outcome::Unknown => unreachable!("a matched adapter never yields unknown"),
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Corroboration, MeetingAdapter};
    use ma_signal::Subject;
    use proptest::prelude::*;

    struct Desktop {
        id: &'static str,
        image: &'static str,
        weight: u8,
    }
    impl MeetingAdapter for Desktop {
        fn id(&self) -> &str {
            self.id
        }
        fn evidence_weight(&self) -> u8 {
            self.weight
        }
        fn corroboration(&self) -> Corroboration {
            Corroboration {
                microphone: true,
                tab: false,
            }
        }
        fn matches(&self, subject: &Subject) -> Option<MatchKind> {
            match subject {
                Subject::Process { image_name, .. } if image_name == self.image => {
                    Some(MatchKind::Process)
                }
                _ => None,
            }
        }
    }
    struct Browser {
        id: &'static str,
        image: &'static str,
        host: &'static str,
    }
    impl MeetingAdapter for Browser {
        fn id(&self) -> &str {
            self.id
        }
        fn evidence_weight(&self) -> u8 {
            1
        }
        fn corroboration(&self) -> Corroboration {
            Corroboration {
                microphone: true,
                tab: true,
            }
        }
        fn matches(&self, subject: &Subject) -> Option<MatchKind> {
            match subject {
                Subject::Process { image_name, .. } if image_name == self.image => {
                    Some(MatchKind::Process)
                }
                Subject::Tab { host, .. } if host == self.host => Some(MatchKind::Tab),
                _ => None,
            }
        }
    }
    struct Panicking;
    impl MeetingAdapter for Panicking {
        fn id(&self) -> &str {
            "panicking"
        }
        fn evidence_weight(&self) -> u8 {
            9
        }
        fn corroboration(&self) -> Corroboration {
            Corroboration {
                microphone: true,
                tab: false,
            }
        }
        fn matches(&self, _: &Subject) -> Option<MatchKind> {
            panic!("adapter bug")
        }
    }

    fn table() -> AdapterTable {
        let mut t = AdapterTable::new(1);
        t.register(Box::new(Desktop {
            id: "desk-a",
            image: "example-desk.exe",
            weight: 2,
        }));
        t.register(Box::new(Desktop {
            id: "desk-b",
            image: "example-other.exe",
            weight: 2,
        }));
        t.register(Box::new(Desktop {
            id: "desk-c",
            image: "example-desk-c.exe",
            weight: 2,
        }));
        t.register(Box::new(Browser {
            id: "browser-x",
            image: "example-browser.exe",
            host: "meet.example.test",
        }));
        t
    }
    fn timeline(text: &str) -> SignalTimeline {
        SignalTimeline::from_jsonl(text).expect("fixture parses")
    }
    const START_END: &str =
        include_str!("../../../fixtures/signal-timelines/desktop-start-end.jsonl");
    const WALL_JUMP: &str =
        include_str!("../../../fixtures/signal-timelines/wall-clock-jump.jsonl");
    const RESYNC: &str = include_str!("../../../fixtures/signal-timelines/resync-start.jsonl");
    const CONCURRENT: &str =
        include_str!("../../../fixtures/signal-timelines/two-concurrent.jsonl");
    const TAB_ONLY: &str =
        include_str!("../../../fixtures/signal-timelines/browser-tab-only.jsonl");
    const TAB_WITH_MIC: &str =
        include_str!("../../../fixtures/signal-timelines/browser-tab-with-mic.jsonl");
    const TAB_FORGED: &str =
        include_str!("../../../fixtures/signal-timelines/browser-tab-forged.jsonl");
    const TAB_CROSS_TREE: &str =
        include_str!("../../../fixtures/signal-timelines/browser-tab-cross-tree.jsonl");

    /// v-win1-same-tree-mic-corroborates: tab and microphone from the same browser tree start.
    #[test]
    fn same_process_tree_mic_and_tab_corroborate() {
        let out = run(TAB_WITH_MIC);
        let start = out
            .decisions
            .iter()
            .find(|d| d.outcome.is_determinate_start())
            .expect("same tree corroborates");
        assert_eq!(start.adapter_id.as_deref(), Some("browser-x"));
        assert_eq!(start.rule_id, "adapter-match");
        assert_eq!(
            start.evidence.len(),
            2,
            "cites the tab and the microphone signal"
        );
        let t = timeline(TAB_WITH_MIC);
        let roots: Vec<Option<u32>> = t
            .signals()
            .iter()
            .filter(|s| start.evidence.contains(&s.signal_id))
            .map(|s| s.payload.process_tree_root_pid)
            .collect();
        assert_eq!(roots, vec![Some(6300), Some(6300)]);
        assert!(out.decisions.iter().all(
            |d| d.rule_id != "process-tree-mismatch" && d.rule_id != "process-tree-root-absent"
        ));
    }

    #[test]
    fn another_browser_tree_cannot_end_the_active_meeting() {
        let foreign_stop = "01990cdf-882c-7000-8000-1b3152f0cc00";
        let joined_stop = "01990cdf-882d-7000-9000-1bcf5480dd00";
        let text = format!(
            "{TAB_WITH_MIC}{{\"signal_id\":\"{foreign_stop}\",\"source_id\":\"os.audio-session\",\"kind\":\"mic_capture_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":7101,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3000000000,\"wall_utc_ms\":1756857603000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":7100}}}}\n{{\"signal_id\":\"{joined_stop}\",\"source_id\":\"os.audio-session\",\"kind\":\"mic_capture_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":6301,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":4000000000,\"wall_utc_ms\":1756857604000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":6300}}}}\n"
        );
        let out = run(&text);
        let ends: Vec<&Decision> = out
            .decisions
            .iter()
            .filter(|d| matches!(d.outcome, Outcome::Determinate { phase: Phase::End }))
            .collect();
        assert_eq!(
            ends.len(),
            1,
            "only the joined tree ends the meeting: {out:?}"
        );
        assert!(ends[0]
            .evidence
            .iter()
            .any(|id| id.to_string() == joined_stop));
        assert!(ends[0]
            .evidence
            .iter()
            .all(|id| id.to_string() != foreign_stop));
    }

    /// v-win1-cross-tree-mic-does-not-corroborate: a microphone in another browser tree is not
    /// this tab's microphone.
    #[test]
    fn mic_use_from_a_different_process_tree_does_not_corroborate() {
        let out = run(TAB_CROSS_TREE);
        assert!(
            out.decisions
                .iter()
                .all(|d| !matches!(d.outcome, Outcome::Determinate { .. })),
            "never a determinate decision: {out:?}"
        );
        let mismatch = out
            .decisions
            .iter()
            .find(|d| d.rule_id == "process-tree-mismatch")
            .expect("the mismatch is named");
        assert!(matches!(mismatch.outcome, Outcome::Inconclusive));
        assert_eq!(mismatch.adapter_id.as_deref(), Some("browser-x"));
        assert_eq!(
            mismatch.evidence.len(),
            2,
            "cites both sides of the failed join"
        );
    }

    #[test]
    fn stopped_tab_tree_is_not_reused_by_a_later_microphone() {
        let text = format!(
            "{TAB_CROSS_TREE}{{\"signal_id\":\"01990ce6-0004-7000-8000-000006cfd75c\",\"source_id\":\"os.process\",\"kind\":\"process_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":6300,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3000000000,\"wall_utc_ms\":1756944003000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":6300}}}}\n{{\"signal_id\":\"01990ce6-0005-7000-9000-00000883c32b\",\"source_id\":\"os.audio_session\",\"kind\":\"mic_capture_started\",\"subject\":{{\"type\":\"process\",\"pid\":6301,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3500000000,\"wall_utc_ms\":1756944003500}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":6300}}}}\n"
        );
        let out = run(&text);
        assert!(
            out.decisions
                .iter()
                .all(|decision| !decision.outcome.is_determinate_start()),
            "stopped tab evidence must not corroborate a later microphone: {out:?}"
        );
    }

    #[test]
    fn stopped_microphone_tree_is_not_reused_by_a_later_tab() {
        let text = format!(
            "{TAB_CROSS_TREE}{{\"signal_id\":\"01990ce6-0006-7000-a000-00000a37af1a\",\"source_id\":\"os.audio_session\",\"kind\":\"mic_capture_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":7101,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3000000000,\"wall_utc_ms\":1756944003000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":7100}}}}\n{{\"signal_id\":\"01990ce6-0007-7000-b000-00000beb9b09\",\"source_id\":\"ext.tabs\",\"kind\":\"tab_meeting_present\",\"subject\":{{\"type\":\"tab\",\"host\":\"meet.example.test\",\"tab_key\":\"tab-18\"}},\"observed_at\":{{\"monotonic_ns\":3500000000,\"wall_utc_ms\":1756944003500}},\"authority\":\"extension\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":7100}}}}\n"
        );
        let out = run(&text);
        assert!(
            out.decisions
                .iter()
                .all(|decision| !decision.outcome.is_determinate_start()),
            "stopped microphone evidence must not corroborate a later tab: {out:?}"
        );
    }

    #[test]
    fn foreign_tree_stop_retains_both_candidate_sides() {
        let text = format!(
            "{TAB_CROSS_TREE}{{\"signal_id\":\"01990ce6-0008-7000-8000-00000d9f86f8\",\"source_id\":\"os.process\",\"kind\":\"process_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":8101,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3000000000,\"wall_utc_ms\":1756944003000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":8100}}}}\n{{\"signal_id\":\"01990ce6-0009-7000-9000-00000f5372e7\",\"source_id\":\"os.audio_session\",\"kind\":\"mic_capture_started\",\"subject\":{{\"type\":\"process\",\"pid\":6301,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3500000000,\"wall_utc_ms\":1756944003500}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":6300}}}}\n"
        );
        let out = run(&text);
        assert!(
            out.decisions
                .iter()
                .any(|decision| decision.outcome.is_determinate_start()),
            "a foreign stop must not erase the live tab side: {out:?}"
        );
    }

    /// v-win1-missing-tree-root-is-inconclusive: a tab report without a tree root cannot be joined.
    #[test]
    fn tab_without_a_process_tree_root_is_inconclusive() {
        let text = TAB_WITH_MIC.replace(r#", "payload": {"process_tree_root_pid": 6300}}"#, "}");
        assert_ne!(text, TAB_WITH_MIC, "the tab root was stripped");
        let out = run(&text);
        assert!(
            out.decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "{out:?}"
        );
        assert!(out
            .decisions
            .iter()
            .any(|d| d.rule_id == "process-tree-root-absent"
                && matches!(d.outcome, Outcome::Inconclusive)));
    }

    /// v-win1-diagnostics-cite-signals: every decision in the committed Phase 1 sidecars cites
    /// signal ids of its own timeline and an adapter rule id, read from the sidecar rather than
    /// re-derived.
    #[test]
    fn decision_cites_signal_ids_for_windows_fixtures() {
        for (name, text, golden) in PHASE1 {
            let committed: DetectorOutput =
                serde_json::from_str(golden).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!committed.decisions.is_empty(), "{name}");
            let ids = timeline(text).ids();
            for d in &committed.decisions {
                assert!(
                    !d.evidence.is_empty(),
                    "{name}: {} cites nothing",
                    d.rule_id
                );
                assert!(
                    d.evidence.iter().all(|id| ids.contains(id)),
                    "{name}: {} cites a signal outside its timeline",
                    d.rule_id
                );
                assert!(!d.rule_id.is_empty(), "{name}");
                if d.outcome.is_determinate_start() {
                    assert!(d.adapter_id.is_some(), "{name}: a start names its adapter");
                }
            }
        }
    }
    const GOLDEN: &str =
        include_str!("../../../fixtures/signal-timelines/desktop-start-end.decisions.json");

    fn run(text: &str) -> DetectorOutput {
        decide(&timeline(text), &DetectorConfig::default(), &mut table())
    }

    #[test]
    fn replay_is_byte_identical() {
        let first = run(START_END).to_canonical_json();
        for _ in 0..100 {
            assert_eq!(
                run(START_END).to_canonical_json(),
                first,
                "repeated replay in one process"
            );
        }
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            println!("GOLDEN-BEGIN\n{first}\nGOLDEN-END");
        }
        // The committed golden was produced by a different process on an earlier run; equality
        // here is the fresh-process and cross-machine check.
        assert_eq!(
            first.trim(),
            GOLDEN.trim(),
            "replay must equal the committed decisions byte for byte"
        );
        let starts = run(START_END)
            .decisions
            .iter()
            .filter(|d| d.outcome.is_determinate_start())
            .count();
        assert_eq!(starts, 1);
    }

    /// The five Phase 1 fixtures (contract-replayable-timeline-fixtures) and their committed
    /// decisions sidecars, which the harness wrote once and which replay must reproduce byte for
    /// byte. `UPDATE_GOLDEN=1` rewrites the sidecars from the current detector.
    const PHASE1: &[(&str, &str, &str)] = &[
        (
            "teams-desktop-session",
            include_str!("../../../fixtures/signal-timelines/teams-desktop-session.jsonl"),
            include_str!("../../../fixtures/signal-timelines/teams-desktop-session.decisions.json"),
        ),
        (
            "slack-huddle-session",
            include_str!("../../../fixtures/signal-timelines/slack-huddle-session.jsonl"),
            include_str!("../../../fixtures/signal-timelines/slack-huddle-session.decisions.json"),
        ),
        (
            "zoom-desktop-session",
            include_str!("../../../fixtures/signal-timelines/zoom-desktop-session.jsonl"),
            include_str!("../../../fixtures/signal-timelines/zoom-desktop-session.decisions.json"),
        ),
        (
            "meet-chrome-with-extension",
            include_str!("../../../fixtures/signal-timelines/meet-chrome-with-extension.jsonl"),
            include_str!(
                "../../../fixtures/signal-timelines/meet-chrome-with-extension.decisions.json"
            ),
        ),
        (
            "meet-chrome-without-extension",
            include_str!("../../../fixtures/signal-timelines/meet-chrome-without-extension.jsonl"),
            include_str!(
                "../../../fixtures/signal-timelines/meet-chrome-without-extension.decisions.json"
            ),
        ),
    ];

    #[test]
    fn windows_fixture_timelines_replay_byte_identical() {
        let update = std::env::var_os("UPDATE_GOLDEN").is_some();
        for (name, text, golden) in PHASE1 {
            let first = run(text).to_canonical_json();
            for _ in 0..20 {
                assert_eq!(
                    run(text).to_canonical_json(),
                    first,
                    "{name}: repeated replay"
                );
            }
            if update {
                // ma-detect is pure (no std::fs): print the golden for the operator to commit;
                // the comparison below still runs, so an update run is never a silent pass.
                println!("GOLDEN-BEGIN {name}\n{first}\nGOLDEN-END {name}");
            }
            assert_eq!(
                first.trim(),
                golden.trim(),
                "{name}: replay must equal the committed decisions byte for byte"
            );
            let out = run(text);
            assert!(!out.decisions.is_empty(), "{name}");
            let ids = timeline(text).ids();
            for d in &out.decisions {
                assert!(
                    d.evidence.iter().all(|id| ids.contains(id)),
                    "{name}: every decision cites signals of its own timeline"
                );
            }
        }
        // The three desktop recordings each start and end exactly one meeting.
        for (name, text, _) in &PHASE1[..3] {
            let out = run(text);
            assert_eq!(
                out.decisions
                    .iter()
                    .filter(|d| d.outcome.is_determinate_start())
                    .count(),
                1,
                "{name}"
            );
            assert_eq!(
                out.decisions
                    .iter()
                    .filter(|d| matches!(d.outcome, Outcome::Determinate { phase: Phase::End }))
                    .count(),
                1,
                "{name}"
            );
        }
        // Without the extension, browser microphone use alone never starts capture.
        let (name, text, _) = PHASE1[4];
        assert!(
            run(text)
                .decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "{name}"
        );
    }

    #[test]
    fn wall_clock_jump_does_not_reorder() {
        assert_eq!(
            run(WALL_JUMP).to_canonical_json(),
            run(START_END).to_canonical_json(),
            "wall_utc never participates in ordering"
        );
    }

    #[test]
    fn every_decision_cites_evidence() {
        let known = timeline(START_END).ids();
        for fixture in [START_END, RESYNC, CONCURRENT, TAB_ONLY, TAB_WITH_MIC] {
            let out = run(fixture);
            assert!(!out.decisions.is_empty(), "each fixture yields decisions");
            let ids = timeline(fixture).ids();
            for d in &out.decisions {
                assert!(
                    !d.evidence.is_empty(),
                    "decision {} cites no signal",
                    d.rule_id
                );
                assert!(
                    d.evidence
                        .iter()
                        .all(|id| ids.contains(id) || known.contains(id)),
                    "evidence must be signals of the timeline"
                );
                assert!(!d.rule_id.is_empty());
            }
        }
    }

    #[test]
    fn resync_signal_never_arms() {
        let out = run(RESYNC);
        assert!(
            out.decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "a resync signal may raise a candidate but never a determinate start: {out:?}"
        );
        assert!(out
            .decisions
            .iter()
            .any(|d| d.rule_id == "resync-no-autostart"
                && matches!(d.outcome, Outcome::Inconclusive)));
    }

    #[test]
    fn extension_signal_alone_is_inconclusive() {
        let out = run(TAB_ONLY);
        assert!(
            out.decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "{out:?}"
        );
        assert!(out
            .decisions
            .iter()
            .any(|d| matches!(d.outcome, Outcome::Inconclusive) && d.rule_id == "extension-alone"));
        let with_mic = run(TAB_WITH_MIC);
        let start = with_mic
            .decisions
            .iter()
            .find(|d| d.outcome.is_determinate_start())
            .expect("tab plus microphone is determinate");
        assert_eq!(start.adapter_id.as_deref(), Some("browser-x"));
        assert_eq!(
            start.evidence.len(),
            2,
            "cites the tab signal and the microphone signal"
        );
    }

    /// v-ext-alone-cannot-start: a forged or replayed extension report, even repeated and even with the
    /// browser process running, never yields a determinate start without an OS microphone signal.
    #[test]
    fn forged_extension_signal_does_not_start_capture() {
        let out = run(TAB_FORGED);
        assert!(
            out.decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "{out:?}"
        );
        assert!(
            out.decisions
                .iter()
                .filter(|d| d.rule_id == "extension-alone")
                .count()
                >= 2,
            "every repeated report is judged, none starts capture: {out:?}"
        );
        assert!(
            out.decisions
                .iter()
                .all(|d| !matches!(d.outcome, Outcome::Determinate { .. })),
            "no determinate decision of any phase from extension evidence alone: {out:?}"
        );
    }

    /// A `tab_audible` report without `tab_meeting_present` plus a browser microphone signal must
    /// not start capture: audibility alone is a landing page, not a meeting.
    #[test]
    fn audible_tab_without_meeting_present_does_not_corroborate() {
        let text = TAB_WITH_MIC
            .lines()
            .filter(|l| !l.contains("tab_meeting_present"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = run(&text);
        assert!(
            out.decisions
                .iter()
                .all(|d| !d.outcome.is_determinate_start()),
            "{out:?}"
        );
        assert!(
            run(TAB_WITH_MIC)
                .decisions
                .iter()
                .any(|d| d.outcome.is_determinate_start()),
            "the full fixture still starts"
        );
    }

    #[test]
    fn concurrent_candidates_yield_one_session() {
        let out = run(CONCURRENT);
        let starts: Vec<_> = out
            .decisions
            .iter()
            .filter(|d| d.outcome.is_determinate_start())
            .collect();
        assert_eq!(
            starts.len(),
            1,
            "exactly one session becomes active: {out:?}"
        );
        assert_eq!(
            starts[0].adapter_id.as_deref(),
            Some("desk-a"),
            "earliest start wins at equal evidence weight"
        );
        let suppressed: Vec<_> = out.decisions.iter().filter(|d| matches!(&d.outcome, Outcome::Conflicting { suppressed: SuppressionReason::LowerPrecedence { active_adapter_id } } if active_adapter_id == "desk-a")).collect();
        assert!(
            !suppressed.is_empty(),
            "the loser is recorded as a suppressed candidate with a reason"
        );
        assert_eq!(suppressed[0].adapter_id.as_deref(), Some("desk-b"));
        let ends = out
            .decisions
            .iter()
            .filter(|d| matches!(d.outcome, Outcome::Determinate { phase: Phase::End }))
            .count();
        assert_eq!(ends, 1);
    }

    #[test]
    fn panicking_adapter_is_disabled_not_fatal() {
        let mut t = table();
        t.register(Box::new(Panicking));
        let out = decide(&timeline(START_END), &DetectorConfig::default(), &mut t);
        assert!(t.is_disabled("panicking"));
        assert_eq!(
            out.diagnostics.len(),
            1,
            "one diagnostic for the disabled adapter"
        );
        assert_eq!(out.diagnostics[0].adapter_id, "panicking");
        assert_eq!(
            out.decisions
                .iter()
                .filter(|d| d.outcome.is_determinate_start())
                .count(),
            1,
            "the pipeline still detects the meeting"
        );
    }

    #[test]
    fn unknown_subject_is_unknown_not_a_start() {
        let text = START_END.replace("example-desk.exe", "unlisted-app.exe");
        let out = run(&text);
        assert!(out
            .decisions
            .iter()
            .all(|d| !d.outcome.is_determinate_start()));
        assert!(out
            .decisions
            .iter()
            .any(|d| matches!(d.outcome, Outcome::Unknown) && d.rule_id == "no-adapter"));
    }

    proptest! {
        #[test]
        fn outcome_partition_is_total(matched in any::<bool>(), met in any::<bool>(), competing in proptest::option::of("[a-z]{1,8}")) {
            let outcome = partition(matched, met, competing.as_deref());
            let arms = [matches!(outcome, Outcome::Determinate { .. }), matches!(outcome, Outcome::Unknown), matches!(outcome, Outcome::Inconclusive), matches!(outcome, Outcome::Conflicting { .. })];
            prop_assert_eq!(arms.iter().filter(|a| **a).count(), 1, "exactly one arm");
            let (is_unknown, is_inconclusive, is_conflicting) = (arms[1], arms[2], arms[3]);
            if !matched { prop_assert!(is_unknown); }
            if matched && !met { prop_assert!(is_inconclusive); }
            if matched && met && competing.is_some() { prop_assert!(is_conflicting); }
            if matched && met && competing.is_none() { prop_assert!(outcome.is_determinate_start()); }
        }
    }

    #[test]
    fn another_browser_tree_cannot_erase_the_pending_candidate() {
        let foreign_stop = "01990cdf-882c-7000-8000-1b3152f0dd00";
        let joined_mic = "01990cdf-882c-7000-8000-1b3152f0dd01";
        let json = format!(
            "{TAB_ONLY}{{\"signal_id\":\"{foreign_stop}\",\"source_id\":\"os.audio-session\",\"kind\":\"mic_capture_stopped\",\"subject\":{{\"type\":\"process\",\"pid\":7101,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":2000000000,\"wall_utc_ms\":1756857602000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":7100}}}}\n{{\"signal_id\":\"{joined_mic}\",\"source_id\":\"os.audio-session\",\"kind\":\"mic_capture_started\",\"subject\":{{\"type\":\"process\",\"pid\":6301,\"image_name\":\"example-browser.exe\",\"package_family_name\":null}},\"observed_at\":{{\"monotonic_ns\":3000000000,\"wall_utc_ms\":1756857603000}},\"authority\":\"os\",\"schema_version\":1,\"payload\":{{\"process_tree_root_pid\":6300}}}}\n"
        );
        let timeline = SignalTimeline::from_jsonl(&json).unwrap();
        let mut table = table();
        let output = decide(&timeline, &DetectorConfig::default(), &mut table);
        assert!(output
            .decisions
            .iter()
            .any(|decision| decision.outcome.is_determinate_start()));
    }
}
