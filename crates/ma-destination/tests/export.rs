//! A fake destination that can die between "created remotely" and "identity recorded".

use ma_core_types::id::TypedId;
use ma_core_types::{ArtifactId, SessionId};
use ma_destination::*;
use std::collections::BTreeMap;

/// Remote objects keyed by remote id, each stamped with the export key.
struct FakeDest {
    id: &'static str,
    host: &'static str,
    objects: BTreeMap<String, String>,
    next: u32,
    creates: u32,
    updates: u32,
    fail_next_with: Option<DestError>,
}

impl FakeDest {
    fn new(id: &'static str, host: &'static str) -> FakeDest {
        FakeDest {
            id,
            host,
            objects: BTreeMap::new(),
            next: 1,
            creates: 0,
            updates: 0,
            fail_next_with: None,
        }
    }
}

impl Destination for FakeDest {
    fn id(&self) -> &str {
        self.id
    }
    fn host(&self) -> &str {
        self.host
    }
    fn find_by_external_id(
        &mut self,
        export_key: &ExportKey,
    ) -> Result<Option<RemoteIdentity>, DestError> {
        Ok(self
            .objects
            .iter()
            .find(|(_, k)| **k == export_key.0)
            .map(|(rid, k)| RemoteIdentity {
                destination_id: self.id.into(),
                remote_id: rid.clone(),
                external_id: k.clone(),
                resumable_session: None,
            }))
    }
    fn create(&mut self, request: &ExportRequest) -> Result<RemoteIdentity, DestError> {
        if let Some(err) = self.fail_next_with.take() {
            return Err(err);
        }
        let rid = format!("obj-{}", self.next);
        self.next += 1;
        self.creates += 1;
        self.objects.insert(rid.clone(), request.key.0.clone());
        Ok(RemoteIdentity {
            destination_id: self.id.into(),
            remote_id: rid,
            external_id: request.key.0.clone(),
            resumable_session: None,
        })
    }
    fn update(
        &mut self,
        identity: &RemoteIdentity,
        _request: &ExportRequest,
    ) -> Result<RemoteIdentity, DestError> {
        self.updates += 1;
        Ok(identity.clone())
    }
}

fn request(dest: &str) -> ExportRequest {
    ExportRequest::new(SessionId::new(), ArtifactId::new(), 1, dest, "cfg-a", 2048)
}

#[test]
fn crash_before_identity_record_reconciles() {
    let mut dest = FakeDest::new("notion", "api.notion.com");
    let mut exporter = Exporter::new(MemoryExportStore::default());
    let req = request("notion");
    // the create call returns, then the process dies before the identity is persisted
    exporter.crash_after_create = true;
    assert_eq!(
        exporter.export(&mut dest, &req, 1),
        Err(ExportError::Crashed)
    );
    assert_eq!(dest.objects.len(), 1, "the remote object exists");
    assert!(
        exporter.store.recorded_identity(&req.key).is_none(),
        "but nothing local knows its id"
    );
    assert_eq!(
        exporter.store.intent_state(&req.key),
        Some(IntentState::Intended),
        "the intent row carries the key"
    );
    // retry: the external-id lookup finds it and reconciles instead of creating a second copy
    exporter.crash_after_create = false;
    let outcome = exporter.export(&mut dest, &req, 2).unwrap();
    assert!(
        matches!(outcome, ExportOutcome::Reconciled(ref id) if id.remote_id == "obj-1"),
        "{outcome:?}"
    );
    assert_eq!(dest.objects.len(), 1, "exactly one remote object");
    assert_eq!(dest.creates, 1);
    assert_eq!(
        exporter
            .store
            .recorded_identity(&req.key)
            .map(|i| i.remote_id),
        Some("obj-1".into())
    );
    assert_eq!(
        exporter.store.intent_state(&req.key),
        Some(IntentState::Committed)
    );
    let audit = exporter.store.audit();
    assert_eq!(
        audit.len(),
        1,
        "the crashed attempt never reached the audit append; the reconciled one did"
    );
    assert_eq!(audit[0].outcome, AuditOutcome::Reconciled);
    assert_eq!(audit[0].host, "api.notion.com");
}

#[test]
fn retry_creates_no_duplicate_remote_object() {
    let mut dest = FakeDest::new("notion", "api.notion.com");
    let mut exporter = Exporter::new(MemoryExportStore::default());
    let req = request("notion");
    assert!(matches!(
        exporter.export(&mut dest, &req, 1).unwrap(),
        ExportOutcome::Created(_)
    ));
    // a manual re-export and a retried export both update the recorded object
    for now in 2..6 {
        assert!(matches!(
            exporter.export(&mut dest, &req, now).unwrap(),
            ExportOutcome::Reconciled(_)
        ));
    }
    assert_eq!(dest.objects.len(), 1);
    assert_eq!((dest.creates, dest.updates), (1, 4));
    // a different artifact version is a different key and a different object
    let mut v2 = req.clone();
    v2.artifact_version = 2;
    v2.key = ExportKey::compute(req.session_id, req.artifact_id, 2, "notion", "cfg-a");
    assert!(matches!(
        exporter.export(&mut dest, &v2, 9).unwrap(),
        ExportOutcome::Created(_)
    ));
    assert_eq!(dest.objects.len(), 2);
    // the queue: 5-attempt cap with the shared schedule, and the 500-entry backlog cap
    let mut q = ExportQueue::default();
    q.enqueue(req.key.clone(), "notion", 0);
    for attempt in 1..=5 {
        q.record_attempt(&req.key, Err(RetryClass::Retryable), 100, 0.0);
        let entry = q.entries.iter().find(|e| e.key == req.key).unwrap();
        if attempt < 5 {
            assert!(
                matches!(entry.status, ExportStatus::Retrying { .. }),
                "{attempt}: {:?}",
                entry.status
            );
        } else {
            assert_eq!(
                entry.status,
                ExportStatus::FailedPermanent {
                    reason: "attempts_exhausted".into()
                }
            );
        }
    }
    assert_eq!(backoff_with_jitter_ms(1, 0.0), Some(1_000));
    assert_eq!(backoff_with_jitter_ms(2, 0.5), Some(4_500));
    let mut q = ExportQueue::default();
    for i in 0..BACKLOG_CAP {
        q.enqueue(ExportKey(format!("k{i:04}")), "notion", i as u64);
    }
    assert!(q.dropped.is_empty());
    q.enqueue(ExportKey("k-over".into()), "notion", 9_999);
    assert_eq!(
        q.dropped,
        [ExportKey("k0000".into())],
        "the oldest never-attempted export is surfaced as dropped"
    );
    assert_eq!(
        q.entries
            .iter()
            .find(|e| e.key.0 == "k0000")
            .unwrap()
            .status,
        ExportStatus::FailedPermanent {
            reason: "backlog_full".into()
        }
    );
    assert_eq!(q.due(0).len(), BACKLOG_CAP);
}

#[test]
fn auth_failure_is_needs_reauthentication() {
    let mut dest = FakeDest::new("drive", "www.googleapis.com");
    let mut exporter = Exporter::new(MemoryExportStore::default());
    let req = request("drive");
    for (status, class) in [
        (401u16, RetryClass::NeedsReauthentication),
        (403, RetryClass::NeedsReauthentication),
        (429, RetryClass::Retryable),
        (503, RetryClass::Retryable),
        (400, RetryClass::Permanent),
        (422, RetryClass::Permanent),
    ] {
        dest.fail_next_with = Some(DestError::Http { status });
        let err = exporter.export(&mut dest, &req, 1).unwrap_err();
        assert_eq!(
            err,
            ExportError::Failed {
                class,
                error: DestError::Http { status }
            },
            "{status}"
        );
    }
    assert_eq!(classify(&DestError::Network), RetryClass::Retryable);
    let mut q = ExportQueue::default();
    q.enqueue(req.key.clone(), "drive", 0);
    q.record_attempt(&req.key, Err(RetryClass::NeedsReauthentication), 1, 0.0);
    assert_eq!(
        q.entries[0].status,
        ExportStatus::NeedsReauthentication { attempts: 1 },
        "surfaced, not scheduled for a blind retry"
    );
    assert!(q.due(u64::MAX).is_empty());
    assert_eq!(dest.objects.len(), 0, "a failed attempt created nothing");
    let audit = exporter.store.audit();
    assert_eq!(
        audit.len(),
        6,
        "every attempt is audited with its typed outcome"
    );
    assert!(audit
        .iter()
        .all(|a| matches!(a.outcome, AuditOutcome::Failed { .. })));
}

#[test]
fn audit_host_is_in_inventory() {
    let inventory = EgressInventory::embedded();
    for host in [
        "api.notion.com",
        "www.googleapis.com",
        "oauth2.googleapis.com",
    ] {
        assert!(inventory.contains(host), "{host}");
    }
    let mut rogue = FakeDest::new("telemetry", "telemetry.example-vendor.test");
    let mut exporter = Exporter::new(MemoryExportStore::default());
    let req = request("telemetry");
    let err = exporter.export(&mut rogue, &req, 1).unwrap_err();
    assert_eq!(
        err,
        ExportError::UndeclaredHost {
            host: "telemetry.example-vendor.test".into()
        }
    );
    assert_eq!(rogue.creates, 0, "rejected before the request");
    assert!(
        exporter.store.intent_state(&req.key).is_none(),
        "no intent, because nothing was going to happen"
    );
    let audit = exporter.store.audit();
    assert_eq!(audit.len(), 1, "the attempt is recorded");
    assert_eq!(audit[0].outcome, AuditOutcome::RejectedUndeclaredHost);
    // containment: every audited host of a successful send is an inventory host
    let mut good = FakeDest::new("notion", "api.notion.com");
    exporter.export(&mut good, &request("notion"), 2).unwrap();
    for record in exporter
        .store
        .audit()
        .iter()
        .filter(|a| a.outcome != AuditOutcome::RejectedUndeclaredHost)
    {
        assert!(inventory.contains(&record.host), "{}", record.host);
    }
    // the record can hold identifiers and counts only
    let json = serde_json::to_value(&exporter.store.audit()[1]).unwrap();
    let keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "artifact_id",
            "at_ms",
            "bytes",
            "destination_id",
            "host",
            "outcome"
        ]
    );
    // the descriptor schema requires the external-id lookup capability
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/destination/destination-descriptor.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let descriptor = serde_json::json!({ "destination_id": "notion", "display_name": "Notion", "egress_hosts": ["api.notion.com"], "credential_kind": "internal_integration_token", "integration_owner": "user_account", "capabilities": { "lookup_by_external_id": true, "update": true, "resumable_upload": false } });
    assert!(validator.is_valid(&descriptor));
    let mut bad = descriptor.clone();
    bad["capabilities"]["lookup_by_external_id"] = serde_json::Value::Bool(false);
    assert!(
        !validator.is_valid(&bad),
        "a destination that cannot find its own objects cannot be described"
    );
    for host in descriptor["egress_hosts"].as_array().unwrap() {
        assert!(inventory.contains(host.as_str().unwrap()));
    }
}

#[test]
fn backlog_cap_holds_when_every_pending_export_was_attempted() {
    let mut q = ExportQueue::default();
    for i in 0..BACKLOG_CAP {
        let key = ExportKey(format!("r{i:04}"));
        q.enqueue(key.clone(), "notion", i as u64);
        q.record_attempt(&key, Err(RetryClass::Retryable), 100, 0.0);
    }
    assert_eq!(q.due(u64::MAX).len(), BACKLOG_CAP);
    q.enqueue(ExportKey("r-over".into()), "notion", 9_999);
    assert_eq!(
        q.dropped,
        [ExportKey("r-over".into())],
        "the export that cannot be taken is surfaced"
    );
    let pending = q
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                ExportStatus::Queued | ExportStatus::Retrying { .. }
            )
        })
        .count();
    assert_eq!(pending, BACKLOG_CAP, "the cap is never exceeded");
    assert_eq!(
        q.entries
            .iter()
            .find(|e| e.key.0 == "r-over")
            .unwrap()
            .status,
        ExportStatus::FailedPermanent {
            reason: "backlog_full".into()
        }
    );
}
