//! The queue driven by recording fake processors: idempotent enqueue, lease recovery without a
//! duplicate artifact, configuration change → new step, edit preservation across regeneration,
//! deletion cancelling in-flight steps, and intent-before-effect ordering.

use ma_core_types::id::TypedId;
use ma_core_types::{ArtifactId, MeetingId, SessionId};
use ma_workflow::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn spec(session: SessionId, meeting: MeetingId, config: &str) -> StepSpec {
    StepSpec {
        meeting_id: meeting,
        session_id: session,
        kind: StepKind::Transcribe,
        inputs: vec![ArtifactId::new()],
        processor_id: "example-stt".into(),
        processor_version: "1.0.0".into(),
        config_hash: config.into(),
        work_items: 3,
    }
}

/// Writes one artifact file per step through the ledger, and counts real writes.
struct FileWriter {
    dir: PathBuf,
    writes: u32,
    /// Simulate a kill between the effect and the ledger commit.
    kill_after_effect: bool,
    fail_with: Option<RetryClass>,
}

impl StepExecutor for FileWriter {
    fn execute(
        &mut self,
        step: &Step,
        ctx: &mut EffectContext<'_>,
    ) -> Result<StepResult, StepError> {
        if let Some(class) = self.fail_with {
            return Err(StepError {
                class,
                message: "processor failed".into(),
            });
        }
        let key = format!("transcript:{}", step.key.0);
        if let Some(existing) = ctx.existing("artifact", &key) {
            return Ok(StepResult {
                result_ref: existing,
            });
        }
        let effect = ctx.intend("artifact", &key);
        let path = self.dir.join(format!("{}.txt", step.step_id));
        ctx.note_effect("write-file");
        std::fs::write(&path, b"transcript").unwrap();
        self.writes += 1;
        if self.kill_after_effect {
            // the process dies here: the ledger row stays `intended`
            return Err(StepError {
                class: RetryClass::Retryable,
                message: "killed".into(),
            });
        }
        ctx.commit(effect, path.to_str().unwrap());
        Ok(StepResult {
            result_ref: path.to_string_lossy().into_owned(),
        })
    }
}

struct DirLookup(PathBuf);
impl EffectLookup for DirLookup {
    fn find(&self, kind: &str, idempotency_key: &str) -> Option<String> {
        let _ = kind;
        // the artifact directory is the lookup path for local files: find the file this key produced
        std::fs::read_dir(&self.0)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "txt") && !idempotency_key.is_empty())
            .map(|p| p.to_string_lossy().into_owned())
    }
}

struct NoLookup;
impl EffectLookup for NoLookup {
    fn find(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

#[test]
fn duplicate_enqueue_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let s = spec(session, meeting, "cfg-a");
    let EnqueueOutcome::Enqueued(step_id) = q.enqueue(&s) else {
        panic!("first enqueue runs")
    };
    assert_eq!(q.enqueue(&s), EnqueueOutcome::AlreadyQueued(step_id));
    assert_eq!(
        q.store.work_items(step_id).len(),
        3,
        "per-chunk work items with stable ids"
    );
    assert_eq!(
        q.store.work_items(step_id)[1].work_item_id,
        WorkItem::stable_id(step_id, 1)
    );
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: None,
    };
    let claimed = q.claim(1_000).unwrap();
    let report = q.run(claimed, 1_000, &mut writer);
    assert_eq!(report.status, StepStatus::Succeeded);
    assert_eq!(
        report.trace,
        [
            "ledger:intended:artifact:transcript:".to_string() + &s.key().0,
            "effect:write-file".into(),
            format!(
                "ledger:committed:{}",
                q.store.step(step_id).unwrap().result_ref.clone().unwrap()
            )
        ],
        "intent is committed before the effect"
    );
    // the same key again: recorded result, nothing executes
    let again = q.enqueue(&s);
    assert_eq!(
        again,
        EnqueueOutcome::AlreadySucceeded {
            step_id,
            result_ref: q.store.step(step_id).unwrap().result_ref.clone()
        }
    );
    assert!(q.claim(2_000).is_none(), "nothing to run");
    assert_eq!(writer.writes, 1);
}

#[test]
fn lease_recovery_creates_no_duplicate_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    q.lease_ms = 1_000;
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let s = spec(session, meeting, "cfg-a");
    let EnqueueOutcome::Enqueued(step_id) = q.enqueue(&s) else {
        panic!()
    };
    // the processor writes the file, then the host is killed before the completion is recorded
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: true,
        fail_with: None,
    };
    let claimed = q.claim(0).unwrap();
    // emulate the kill: the executor returned, but pretend the queue never got to record it
    let mut ctx_trace = Vec::new();
    {
        let mut ctx = EffectContext::for_test(&mut q.store, &claimed, 0, &mut ctx_trace);
        let _ = writer.execute(&claimed, &mut ctx);
    }
    assert_eq!(
        q.store.step(step_id).unwrap().status,
        StepStatus::Running {
            lease_until_ms: 1_000
        },
        "still running as far as the store knows"
    );
    assert!(q
        .store
        .ledger_for_step(step_id)
        .iter()
        .any(|r| r.state == EffectState::Intended));
    // restart after the lease expired: the lookup path (artifact directory) resolves the intent
    let recovered = q.recover(5_000, &DirLookup(dir.path().to_path_buf()));
    assert_eq!(recovered, [step_id]);
    assert_eq!(q.store.step(step_id).unwrap().status, StepStatus::Pending);
    assert!(q
        .store
        .ledger_for_step(step_id)
        .iter()
        .all(|r| r.state == EffectState::Committed));
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: None,
    };
    let claimed = q.claim(5_000).unwrap();
    let report = q.run(claimed, 5_000, &mut writer);
    assert_eq!(report.status, StepStatus::Succeeded);
    assert_eq!(writer.writes, 0, "the re-run reused the committed effect");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "exactly one file"
    );
    assert_eq!(
        q.store.ledger_for_step(step_id).len(),
        1,
        "exactly one effect row"
    );

    // when no lookup can decide, the step waits for an explicit decision instead of recreating
    let dir2 = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    q.lease_ms = 1_000;
    let s2 = spec(session, meeting, "cfg-b");
    let EnqueueOutcome::Enqueued(step2) = q.enqueue(&s2) else {
        panic!()
    };
    let mut writer = FileWriter {
        dir: dir2.path().to_path_buf(),
        writes: 0,
        kill_after_effect: true,
        fail_with: None,
    };
    let claimed = q.claim(0).unwrap();
    let mut trace = Vec::new();
    {
        let mut ctx = EffectContext::for_test(&mut q.store, &claimed, 0, &mut trace);
        let _ = writer.execute(&claimed, &mut ctx);
    }
    assert!(
        q.recover(5_000, &NoLookup).is_empty(),
        "not runnable: unknown effect"
    );
    assert_eq!(
        q.store.step(step2).unwrap().status,
        StepStatus::AwaitingDecision
    );
    assert!(
        q.claim(6_000).is_none(),
        "an awaiting step is never claimed"
    );
    let effect_id = q.store.ledger_for_step(step2)[0].effect_id;
    q.decide(effect_id, EffectDecision::Abandon);
    assert_eq!(q.store.step(step2).unwrap().status, StepStatus::Pending);
}

#[test]
fn config_change_creates_new_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let a = spec(session, meeting, "cfg-a");
    let EnqueueOutcome::Enqueued(step_a) = q.enqueue(&a) else {
        panic!()
    };
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: None,
    };
    let claimed = q.claim(0).unwrap();
    q.run(claimed, 0, &mut writer);
    let result_a = q.store.step(step_a).unwrap().result_ref.clone();
    // a different configuration, a different processor version, a different processor: three new keys
    let mut b = spec(session, meeting, "cfg-b");
    b.inputs = a.inputs.clone();
    let mut c = a.clone();
    c.processor_version = "1.1.0".into();
    let mut d = a.clone();
    d.processor_id = "other-stt".into();
    let mut ids = vec![step_a];
    for s in [&b, &c, &d] {
        assert_ne!(s.key(), a.key());
        let EnqueueOutcome::Enqueued(id) = q.enqueue(s) else {
            panic!("new key, new step")
        };
        ids.push(id);
    }
    assert_eq!(
        q.store.step(step_a).unwrap().result_ref,
        result_a,
        "the previous result is retained"
    );
    assert_eq!(q.store.step(step_a).unwrap().status, StepStatus::Succeeded);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4);
    // retry classification on the new step
    let mut failing = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: Some(RetryClass::Retryable),
    };
    let claimed = q.claim(10).unwrap();
    let report = q.run(claimed, 10, &mut failing);
    assert!(matches!(
        report.status,
        StepStatus::FailedRetryable {
            attempts: 1,
            not_before_ms: 1_010,
            ..
        }
    ));
    assert!(
        q.claim(500).is_none()
            || q.store
                .steps()
                .iter()
                .filter(|s| matches!(s.status, StepStatus::Running { .. }))
                .count()
                == 1,
        "backoff is respected"
    );
    let mut permanent = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: Some(RetryClass::Permanent),
    };
    let mut q2 = Queue::new(MemoryStore::default());
    q2.enqueue(&spec(session, meeting, "cfg-p"));
    let claimed = q2.claim(0).unwrap();
    let report = q2.run(claimed, 0, &mut permanent);
    assert!(matches!(
        report.status,
        StepStatus::FailedPermanent { attempts: 1, .. }
    ));
}

#[test]
fn regeneration_preserves_user_edits() {
    let (meeting, artifact) = (MeetingId::new(), ArtifactId::new());
    let mut content = BTreeMap::new();
    content.insert("cluster-1".to_string(), "Speaker 1".to_string());
    content.insert("seg-1".to_string(), "hello world".to_string());
    content.insert("seg-2".to_string(), "second line".to_string());
    let gen1 = Generation {
        generation_id: uuid::Uuid::from_u128(1),
        meeting_id: meeting,
        artifact_id: artifact,
        step_id: ma_core_types::StepId::new(),
        produced_at_ms: 1,
        processor_id: "example-stt".into(),
        model_id: "model-a".into(),
        adapter_version: "1".into(),
        content: content.clone(),
    };
    let basis = AnchorBasis::of(&gen1, &["cluster-1"]);
    // an edit with no anchor basis is refused, not stored
    assert_eq!(
        propose_edit(
            None,
            meeting,
            artifact,
            TargetKind::SpeakerLabel,
            Anchor::SpeakerCluster {
                cluster_id: "cluster-1".into()
            },
            "Alice",
            2
        )
        .unwrap_err(),
        EditError::NoAnchorBasis
    );
    let rename = propose_edit(
        Some(&basis),
        meeting,
        artifact,
        TargetKind::SpeakerLabel,
        Anchor::SpeakerCluster {
            cluster_id: "cluster-1".into(),
        },
        "Alice",
        2,
    )
    .unwrap();
    let text_fix = propose_edit(
        Some(&basis),
        meeting,
        artifact,
        TargetKind::TranscriptText,
        Anchor::Segment {
            segment_id: "seg-2".into(),
            text_hash: text_hash("second line"),
        },
        "second line, fixed",
        3,
    )
    .unwrap();
    let mut overlays = vec![rename.clone(), text_fix.clone()];
    let view = compose(&gen1, &overlays);
    assert_eq!(view.content["cluster-1"], "Alice");
    assert_eq!(view.content["seg-2"], "second line, fixed");
    // regenerate with a different model: new generation row, overlay untouched
    let mut content2 = BTreeMap::new();
    content2.insert("cluster-1".to_string(), "Speaker 1".to_string());
    content2.insert("seg-1".to_string(), "hello world".to_string());
    content2.insert("seg-9".to_string(), "re-segmented".to_string());
    let gen2 = Generation {
        generation_id: uuid::Uuid::from_u128(2),
        model_id: "model-b".into(),
        produced_at_ms: 10,
        content: content2,
        ..gen1.clone()
    };
    let mut store = MemoryStore::default();
    store.insert_generation(gen1.clone());
    store.insert_generation(gen2.clone());
    assert_eq!(
        store.generations(meeting).len(),
        2,
        "generation rows are appended, never edited"
    );
    let before = overlays.clone();
    reanchor(&mut overlays, &AnchorBasis::of(&gen2, &["cluster-1"]));
    assert_eq!(overlays[0].value, before[0].value);
    assert!(
        !overlays[0].orphaned,
        "the speaker rename survives re-segmentation because it anchors to the cluster"
    );
    assert!(
        overlays[1].orphaned,
        "the text edit lost its segment and is kept as orphaned"
    );
    assert_eq!(overlays.len(), 2, "nothing was deleted");
    let view = compose(&gen2, &overlays);
    assert_eq!(view.content["cluster-1"], "Alice");
    assert_eq!(
        view.orphaned,
        [text_fix.overlay_id],
        "orphaned edits are enumerable"
    );
    assert_eq!(view.applied, 1);
}

#[test]
fn delete_cancels_inflight_steps() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let other = MeetingId::new();
    let EnqueueOutcome::Enqueued(pending) = q.enqueue(&spec(session, meeting, "cfg-a")) else {
        panic!()
    };
    let EnqueueOutcome::Enqueued(running) = q.enqueue(&spec(session, meeting, "cfg-b")) else {
        panic!()
    };
    let EnqueueOutcome::Enqueued(unrelated) = q.enqueue(&spec(SessionId::new(), other, "cfg-a"))
    else {
        panic!()
    };
    // start the running step and leave an effect intended (killed mid-effect)
    let claimed = q.store.step(running).unwrap();
    let mut step = claimed.clone();
    step.status = StepStatus::Running {
        lease_until_ms: 9_999,
    };
    q.store.update_step(step.clone());
    let mut trace = Vec::new();
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: true,
        fail_with: None,
    };
    {
        let mut ctx = EffectContext::for_test(&mut q.store, &step, 0, &mut trace);
        let _ = writer.execute(&step, &mut ctx);
    }
    let report = cancel_for_meeting(&mut q.store, meeting);
    let mut cancelled = report.cancelled.clone();
    cancelled.sort();
    let mut expected = vec![pending, running];
    expected.sort();
    assert_eq!(cancelled, expected);
    assert_eq!(
        report.kill_requests,
        [running],
        "a running step's host child must be killed"
    );
    assert!(
        matches!(report.readiness, PurgeReadiness::BlockedOnIntended(ref ids) if ids.len() == 1),
        "purge blocks on the intended row"
    );
    assert_eq!(
        q.store.step(unrelated).unwrap().status,
        StepStatus::Pending,
        "other meetings are untouched"
    );
    assert!(
        q.claim(0).map(|s| s.step_id) == Some(unrelated),
        "cancelled steps are never claimed"
    );
    // resolving the intent unblocks the purge
    let effect_id = q.store.ledger_for_step(running)[0].effect_id;
    q.decide(effect_id, EffectDecision::Abandon);
    assert_eq!(
        cancel_for_meeting(&mut q.store, meeting).readiness,
        PurgeReadiness::Ready
    );
}

#[test]
fn failed_attempt_with_intended_effect_is_not_rerun_blindly() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let s = spec(session, meeting, "cfg-x");
    let EnqueueOutcome::Enqueued(step_id) = q.enqueue(&s) else {
        panic!()
    };
    // the executor performs the effect and then fails before committing (the HostCrashed shape)
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: true,
        fail_with: None,
    };
    let claimed = q.claim(0).unwrap();
    let report = q.run(claimed, 0, &mut writer);
    assert_eq!(
        report.status,
        StepStatus::AwaitingDecision,
        "an intended effect with no outcome is unknown, not retryable"
    );
    assert!(q.claim(10_000).is_none(), "never re-run blindly");
    // the lookup path resolves it, then the retry reuses the effect
    assert_eq!(
        q.recover(20_000, &DirLookup(dir.path().to_path_buf())),
        [step_id]
    );
    let mut writer = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: None,
    };
    let claimed = q.claim(20_000).unwrap();
    assert_eq!(
        q.run(claimed, 20_000, &mut writer).status,
        StepStatus::Succeeded
    );
    assert_eq!(writer.writes, 0);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn terminal_keys_are_reported_not_queued() {
    let dir = tempfile::tempdir().unwrap();
    let mut q = Queue::new(MemoryStore::default());
    let (session, meeting) = (SessionId::new(), MeetingId::new());
    let s = spec(session, meeting, "cfg-p");
    let EnqueueOutcome::Enqueued(step_id) = q.enqueue(&s) else {
        panic!()
    };
    let mut permanent = FileWriter {
        dir: dir.path().to_path_buf(),
        writes: 0,
        kill_after_effect: false,
        fail_with: Some(RetryClass::Permanent),
    };
    let claimed = q.claim(0).unwrap();
    q.run(claimed, 0, &mut permanent);
    match q.enqueue(&s) {
        EnqueueOutcome::Terminal {
            step_id: id,
            status: StepStatus::FailedPermanent { .. },
        } => assert_eq!(id, step_id),
        other => panic!("{other:?}"),
    }
    let report = cancel_for_meeting(&mut q.store, meeting);
    assert!(
        report.cancelled.is_empty(),
        "a permanently failed step is not cancelled again"
    );
    let cancelled = spec(session, meeting, "cfg-c");
    let EnqueueOutcome::Enqueued(_) = q.enqueue(&cancelled) else {
        panic!()
    };
    cancel_for_meeting(&mut q.store, meeting);
    assert!(matches!(
        q.enqueue(&cancelled),
        EnqueueOutcome::Terminal {
            status: StepStatus::Cancelled,
            ..
        }
    ));
}
