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
    tab: Option<SignalId>,
    resync: bool,
    evidence: Vec<SignalId>,
}

#[derive(Debug, Clone)]
struct Active {
    subject_key: Option<String>,
    started_at: u64,
    weight: u8,
    evidence: Vec<SignalId>,
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
                candidates.remove(&adapter_id);
                if let Some(meeting) = active.get(&adapter_id) {
                    if meeting.subject_key.as_deref() == Some(subject_key.as_str())
                        || kind == MatchKind::Process
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
            candidate.evidence.push(signal.signal_id);
            match (signal.kind, signal.authority, kind) {
                (SignalKind::MicCaptureStarted, Authority::Os, MatchKind::Process) => {
                    candidate.microphone = Some(signal.signal_id);
                    candidate.subject_key.get_or_insert(subject_key.clone());
                    if signal.payload.restart_resync {
                        candidate.resync = true;
                    }
                }
                // only a meeting-present report corroborates; audibility alone is a landing page
                (SignalKind::TabMeetingPresent, Authority::Extension, MatchKind::Tab) => {
                    candidate.tab = Some(signal.signal_id);
                    if candidate.subject_key.is_none() {
                        candidate.subject_key = Some(subject_key.clone());
                    }
                }
                _ => {}
            }
            let met = (!needs.microphone || candidate.microphone.is_some())
                && (!needs.tab || candidate.tab.is_some());
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
}
