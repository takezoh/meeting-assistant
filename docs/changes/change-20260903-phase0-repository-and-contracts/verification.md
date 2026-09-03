---
change: change-20260903-phase0-repository-and-contracts
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

Phase 0 is complete when every success condition below has evidence from a named, executable check. A check that is written but never run is treated as a failing check, which is what `contract-verification-tiering` exists to enforce.

## Success conditions

| # | condition | evidence |
| --- | --- | --- |
| S-1 | The four PLAN Phase 0 exit criteria each have a check that fails when the criterion is violated | A-01, A-02, A-03, A-08, A-12 all green in their tiers |
| S-2 | Meeting-service-specific logic cannot reach the workflow core | `cargo xtask boundary` green on the clean workspace and the negative fixture reporting exactly its three planted violations and none of its three decoys |
| S-3 | The capture engine continues recording after the interface terminates, and after a processor crash | `v-topology-ui-kill` and `v-isolation-processor-abort-keeps-recording`, both in the windows tier |
| S-4 | Workflow steps and artifacts have stable identifiers and states across a crash | `v-identity-recovery-reuse`, `v-workflow-lease-recovery-no-duplicate`, `v-export-crash-before-identity-record` |
| S-5 | Core boundaries require no proprietary backend | `v-egress-inventory-complete` and `v-egress-inventory-no-first-party` |
| S-6 | Nothing a user cancels leaves audio on disk, and nothing a user deletes leaves anything but a tombstone | `v-consent-cancel-leaves-no-audio-byte`, `v-purge-completeness`, `v-purge-idempotent` |
| S-7 | No secret and no meeting content reaches any file the application writes | `v-redaction-marker-scan`, `v-redaction-content-type-not-loggable`, `v-credential-no-secret-in-any-written-file` |
| S-8 | Every decision this phase makes is recorded in a schema-conformant, indexed document | `dev-docs conformance --root docs` and `dev-docs lint --kind adr` |
| S-9 | Every declared verification runs somewhere, and the tier that needs Windows has actually run | `v-tier-every-t2-registered` and a green `v-tier-windows-suite-green` |

## Verification tiers

Two tiers, both declared in `verification-tiers.toml`. Registration is mandatory: a verification identifier
absent from the file, or assigned to the portable tier while its test is Windows-annotated, fails the build.

**portable** — runs on any host including the Linux development host, on every push and pull request, and
blocks merge. Holds every T0 and T1 verification plus the build and test of the contract-core crates
(`ma-core-types`, `ma-session`, `ma-signal`, `ma-detect`, `ma-store`, `ma-workflow`, `ma-processor`,
`ma-destination`, `ma-manifest`, the core module of `ma-secure`, and `xtask`).

**windows** — runs on a Windows 11 runner, on every pull request into the default branch and nightly.
Holds every T2. **Phase 0 is not complete until this job reports green.** A windows job that cannot acquire
a runner fails; it never passes by absence.

### T0

Static and mechanical: schema validation, boundary and inventory lint, cargo metadata, compile-time assertions, frontmatter conformance. No product behaviour executes. Portable tier.

| verification | command | contract |
| --- | --- | --- |
| `v-addressing-db-not-relocatable` | `cargo test -p ma-store database_path_is_pinned_to_local_appdata` | `contract-artifact-addressing` |
| `v-addressing-no-absolute-paths` | `cargo test -p ma-store no_absolute_artifact_path_is_stored` | `contract-artifact-addressing` |
| `v-boundary-ci-gate` | `cargo test -p xtask ci_workflow_invokes_boundary_and_deny` | `contract-module-boundary-enforcement` |
| `v-boundary-clean-workspace` | `cargo xtask boundary` | `contract-module-boundary-enforcement` |
| `v-credential-type-not-displayable` | `cargo test -p ma-secure --test compile_fail secret_cannot_be_displayed` | `contract-credential-custody` |
| `v-detect-purity-lint` | `cargo xtask boundary --check forbidden-imports` | `contract-detector-determinism` |
| `v-docs-adr-placement` | `dev-docs lint --kind adr` | `contract-docs-conformance` |
| `v-docs-change-members-nonempty` | `dev-docs conformance --change change-20260903-phase0-repository-and-contracts` | `contract-docs-conformance` |
| `v-docs-invariant-enforcement-named` | `dev-docs lint --kind design` | `contract-docs-conformance` |
| `v-docs-schema-conformance` | `dev-docs conformance --root docs` | `contract-docs-conformance` |
| `v-egress-inventory-complete` | `cargo test -p ma-secure --test egress_inventory every_source_host_is_declared` | `contract-egress-inventory` |
| `v-egress-inventory-no-first-party` | `cargo test -p ma-secure --test egress_inventory owners_are_closed_and_entries_are_reachable` | `contract-egress-inventory` |
| `v-ipc-schema-conformance` | `cargo test -p ma-ipc schema_golden_roundtrip` | `contract-ipc-protocol` |
| `v-isolation-capture-path-edges` | `cargo xtask boundary --rule capture-path-isolation` | `contract-processing-isolation` |
| `v-isolation-native-link-confined` | `cargo xtask boundary --rule native-inference-confinement` | `contract-processing-isolation` |
| `v-redaction-content-type-not-loggable` | `cargo test -p ma-secure --test compile_fail content_type_cannot_be_logged` | `contract-diagnostic-redaction` |
| `v-session-table-conformance` | `cargo test -p ma-session transition_table_matches_contract_json` | `contract-session-state-machine` |
| `v-signal-no-ui-text-fields` | `cargo test -p ma-signal schema_contains_no_free_text_subject` | `contract-signal-envelope` |
| `v-signal-schema-conformance` | `cargo test -p ma-signal schema_golden_roundtrip` | `contract-signal-envelope` |
| `v-store-wal-config` | `cargo test -p ma-store wal_and_busy_timeout_configured` | `contract-store-ownership` |
| `v-tier-ci-defines-both-gates` | `cargo test -p xtask ci_defines_portable_and_windows_gates` | `contract-verification-tiering` |
| `v-tier-every-t2-registered` | `cargo xtask verify --check-registration` | `contract-verification-tiering` |
| `v-tier-portable-suite-on-non-windows` | `cargo xtask verify --tier portable` | `contract-verification-tiering` |

### T1

Deterministic in-process automated tests against seams and fakes. Portable tier.

| verification | command | contract |
| --- | --- | --- |
| `v-addressing-identifier-only-segments` | `cargo test -p ma-store hostile_titles_never_reach_the_filesystem` | `contract-artifact-addressing` |
| `v-addressing-relocation` | `cargo test -p ma-store root_relocation_preserves_references` | `contract-artifact-addressing` |
| `v-authz-build-channel-carveout` | `cargo test -p ma-ipc authz_build_channel_carveout` | `contract-ipc-transport-authz` |
| `v-authz-dacl-shape` | `cargo test -p ma-secure acl_pipe_descriptor_owner_only` | `contract-ipc-transport-authz` |
| `v-authz-foreign-sid-rejected` | `cargo test -p ma-ipc authz_foreign_sid_rejected_before_dispatch` | `contract-ipc-transport-authz` |
| `v-boundary-feature-gated-leak` | `cargo test -p xtask feature_gated_adapter_edge_is_detected` | `contract-module-boundary-enforcement` |
| `v-boundary-negative-fixture` | `cargo test -p xtask boundary_negative_fixture_reports_three_violations` | `contract-module-boundary-enforcement` |
| `v-budget-cancellation-bound` | `cargo test -p ma-processor cancellation_observed_within_bound` | `contract-processor-budget` |
| `v-budget-cost-convergence` | `cargo test -p ma-processor per_item_cost_does_not_grow` | `contract-processor-budget` |
| `v-budget-overrun-is-warning` | `cargo test -p ma-processor budget_overrun_emits_warning_not_failure` | `contract-processor-budget` |
| `v-budget-progress-monotonic` | `cargo test -p ma-processor progress_is_monotonic` | `contract-processor-budget` |
| `v-chunk-backpressure-gap` | `cargo test -p ma-capture stalling_filesystem_yields_gap_not_stall` | `contract-chunk-durability` |
| `v-chunk-manifest-vs-directory` | `cargo test -p ma-capture directory_is_truth_manifest_is_cache` | `contract-chunk-durability` |
| `v-consolidate-crash-idempotent` | `cargo test -p ma-capture crash_between_verify_and_delete_is_idempotent` | `contract-track-consolidation` |
| `v-consolidate-lossless` | `cargo test -p ma-capture flac_decodes_sample_identical` | `contract-track-consolidation` |
| `v-consolidate-mismatch-keeps-chunks` | `cargo test -p ma-capture verification_mismatch_preserves_chunks` | `contract-track-consolidation` |
| `v-credential-argv-free` | `cargo test -p ma-processor secret_never_appears_in_child_argv` | `contract-credential-custody` |
| `v-credential-missing-is-typed` | `cargo test -p ma-secure missing_credential_is_needs_authentication` | `contract-credential-custody` |
| `v-detect-adapter-panic-isolated` | `cargo test -p ma-detect panicking_adapter_is_disabled_not_fatal` | `contract-detector-outcome-partition` |
| `v-detect-conflict-precedence` | `cargo test -p ma-detect concurrent_candidates_yield_one_session` | `contract-detector-outcome-partition` |
| `v-detect-evidence-present` | `cargo test -p ma-detect every_decision_cites_evidence` | `contract-detector-determinism` |
| `v-detect-extension-alone-inconclusive` | `cargo test -p ma-detect extension_signal_alone_is_inconclusive` | `contract-detector-outcome-partition` |
| `v-detect-partition-exhaustive` | `cargo test -p ma-detect outcome_partition_is_total` | `contract-detector-outcome-partition` |
| `v-detect-replay-determinism` | `cargo test -p ma-detect replay_is_byte_identical` | `contract-detector-determinism` |
| `v-egress-audit-matches-inventory` | `cargo test -p ma-destination audit_host_is_in_inventory` | `contract-egress-inventory` |
| `v-egress-inventory-negative-fixture` | `cargo test -p ma-secure --test egress_inventory undeclared_host_is_named` | `contract-egress-inventory` |
| `v-export-auth-failure-classification` | `cargo test -p ma-destination auth_failure_is_needs_reauthentication` | `contract-destination-export-idempotency` |
| `v-export-crash-before-identity-record` | `cargo test -p ma-destination crash_before_identity_record_reconciles` | `contract-destination-export-idempotency` |
| `v-export-duplicate-retry-no-duplicate` | `cargo test -p ma-destination retry_creates_no_duplicate_remote_object` | `contract-destination-export-idempotency` |
| `v-ext-alone-cannot-start` | `cargo test -p ma-detect forged_extension_signal_does_not_start_capture` | `contract-extension-channel-trust` |
| `v-ext-origin-rejects-web` | `cargo test -p ma-ext-channel web_origin_rejected` | `contract-extension-channel-trust` |
| `v-ext-replay-rejected` | `cargo test -p ma-ext-channel stale_sequence_rejected` | `contract-extension-channel-trust` |
| `v-ext-token-required` | `cargo test -p ma-ext-channel request_without_token_rejected` | `contract-extension-channel-trust` |
| `v-identity-cross-surface-equality` | `cargo test -p ma-store identifier_identical_across_db_path_and_export` | `contract-stable-identity` |
| `v-identity-ordering` | `cargo test -p ma-core-types uuidv7_is_time_ordered` | `contract-stable-identity` |
| `v-ipc-handshake-mismatch` | `cargo test -p ma-ipc handshake_major_mismatch_refused` | `contract-ipc-protocol` |
| `v-ipc-resync-after-stall` | `cargo test -p ma-ipc stalled_client_resyncs` | `contract-ipc-protocol` |
| `v-isolation-negative-fixture` | `cargo test -p xtask isolation_negative_fixture` | `contract-processing-isolation` |
| `v-manifest-digest-mismatch-no-activation` | `cargo test -p ma-manifest digest_mismatch_blocks_adapter_activation` | `contract-release-manifest-trust` |
| `v-manifest-downgrade-rejected` | `cargo test -p ma-manifest replayed_older_manifest_rejected` | `contract-release-manifest-trust` |
| `v-manifest-tampered-rejected` | `cargo test -p ma-manifest tampered_manifest_rejected` | `contract-release-manifest-trust` |
| `v-manifest-unknown-key-rejected` | `cargo test -p ma-manifest unknown_key_rejected_rollover_accepted` | `contract-release-manifest-trust` |
| `v-mode-countdown-cancel-suppression` | `cargo test -p ma-session cancel_suppresses_rearm_for_identity` | `contract-recording-mode-policy` |
| `v-mode-hysteresis-flap` | `cargo test -p ma-session flapping_end_signal_yields_one_session` | `contract-recording-mode-policy` |
| `v-mode-resolution-order` | `cargo test -p ma-session mode_resolution_override_class_global` | `contract-recording-mode-policy` |
| `v-mode-suspend-resume-reevaluation` | `cargo test -p ma-session suspend_during_countdown_reevaluates` | `contract-recording-mode-policy` |
| `v-processor-argv-no-shell` | `cargo test -p ma-processor config_value_never_reaches_a_shell` | `contract-processor-interface` |
| `v-processor-capability-refusal` | `cargo test -p ma-processor unsupported_language_is_typed_refusal` | `contract-processor-interface` |
| `v-processor-model-digest` | `cargo test -p ma-processor model_digest_mismatch_is_permanent_failure` | `contract-processor-interface` |
| `v-processor-staging-exact-contents` | `cargo test -p ma-processor staging_dir_contains_only_declared_inputs` | `contract-processor-interface` |
| `v-purge-cancels-inflight-steps` | `cargo test -p ma-workflow delete_cancels_inflight_steps` | `contract-retention-purge` |
| `v-purge-completeness` | `cargo test -p ma-store purge_leaves_only_tombstone` | `contract-retention-purge` |
| `v-purge-idempotent` | `cargo test -p ma-store purge_rerun_is_idempotent` | `contract-retention-purge` |
| `v-redaction-error-display-elides-payload` | `cargo test -p ma-secure parse_error_display_elides_payload` | `contract-diagnostic-redaction` |
| `v-session-crash-in-arming` | `cargo test -p ma-session recovery_from_arming_lands_in_idle` | `contract-session-state-machine` |
| `v-session-exhaustive-step` | `cargo test -p ma-session step_is_total_over_state_event_space` | `contract-session-state-machine` |
| `v-session-idempotent-commands` | `cargo test -p ma-session repeated_stop_is_success_without_effect` | `contract-session-state-machine` |
| `v-signal-resync-no-autostart` | `cargo test -p ma-detect resync_signal_never_arms` | `contract-signal-envelope` |
| `v-signal-wall-clock-jump` | `cargo test -p ma-detect wall_clock_jump_does_not_reorder` | `contract-signal-envelope` |
| `v-store-migration-forward-from-every-version` | `cargo test -p ma-store migrate_from_every_released_version` | `contract-store-ownership` |
| `v-store-role-enforcement` | `cargo test -p ma-store write_outside_role_family_is_rejected` | `contract-store-ownership` |
| `v-timeline-coverage-invariant` | `cargo test -p ma-core-types chunks_and_gaps_tile_without_overlap` | `contract-session-timeline` |
| `v-timeline-format-change-segment` | `cargo test -p ma-capture device_format_change_opens_new_segment` | `contract-session-timeline` |
| `v-timeline-gap-preserving-timestamps` | `cargo test -p ma-core-types timestamps_survive_missing_chunk` | `contract-session-timeline` |
| `v-timeline-track-independence` | `cargo test -p ma-core-types tracks_have_independent_origins` | `contract-session-timeline` |
| `v-workflow-config-change-new-step` | `cargo test -p ma-workflow config_change_creates_new_step` | `contract-workflow-step-idempotency` |
| `v-workflow-duplicate-enqueue-noop` | `cargo test -p ma-workflow duplicate_enqueue_is_noop` | `contract-workflow-step-idempotency` |
| `v-workflow-edit-preservation` | `cargo test -p ma-workflow regeneration_preserves_user_edits` | `contract-workflow-step-idempotency` |
| `v-workflow-lease-recovery-no-duplicate` | `cargo test -p ma-workflow lease_recovery_creates_no_duplicate_artifact` | `contract-workflow-step-idempotency` |

### T2

Multi-process or system integration: process kills, real named pipes, real access control lists, real filesystem timing. Windows tier.

| verification | command | contract |
| --- | --- | --- |
| `v-authz-pipe-squat` | `cargo test -p ma-engine --test topology pipe_squat_detected` | `contract-ipc-transport-authz` |
| `v-chunk-2h-scale` | `cargo test -p ma-engine --test durability two_hour_recovery_within_bound` | `contract-chunk-durability` |
| `v-chunk-kill-recovery` | `cargo test -p ma-engine --test durability kill_mid_chunk_bounded_loss` | `contract-chunk-durability` |
| `v-consent-cancel-leaves-no-audio-byte` | `cargo test -p ma-engine --test consent cancelled_countdown_writes_no_audio` | `contract-consent-surface-precondition` |
| `v-consent-engine-notification-starts-without-client` | `cargo test -p ma-engine --test consent auto_start_with_no_client_attached` | `contract-consent-surface-precondition` |
| `v-consent-no-surface-no-start` | `cargo test -p ma-engine --test consent no_surface_no_capture` | `contract-consent-surface-precondition` |
| `v-consent-surface-loss-keeps-recording` | `cargo test -p ma-engine --test consent surface_loss_keeps_recording` | `contract-consent-surface-precondition` |
| `v-credential-no-secret-in-any-written-file` | `cargo test -p ma-secure --test leak_scan planted_secret_absent_from_all_written_files` | `contract-credential-custody` |
| `v-export-offline-queue-survives-restart` | `cargo test -p ma-workflow --test export_queue queue_survives_restart` | `contract-destination-export-idempotency` |
| `v-identity-recovery-reuse` | `cargo test -p ma-engine --test durability recovery_reuses_session_id` | `contract-stable-identity` |
| `v-ipc-backpressure-never-stalls-capture` | `cargo test -p ma-engine --test topology wedged_client_does_not_stall_capture` | `contract-ipc-protocol` |
| `v-isolation-processor-abort-keeps-recording` | `cargo test -p ma-engine --test durability processor_abort_keeps_recording` | `contract-processing-isolation` |
| `v-purge-interrupted-resumes` | `cargo test -p ma-engine --test durability killed_purge_resumes_to_completion` | `contract-retention-purge` |
| `v-redaction-marker-scan` | `cargo test -p ma-secure --test leak_scan diagnostic_bundle_contains_no_markers` | `contract-diagnostic-redaction` |
| `v-store-busy-does-not-stall-capture` | `cargo test -p ma-engine --test durability wedged_writer_does_not_stall_capture` | `contract-store-ownership` |
| `v-tier-windows-suite-green` | `cargo xtask verify --tier windows` | `contract-verification-tiering` |
| `v-topology-engine-restart-resync` | `cargo test -p ma-engine --test topology engine_restart_client_resync` | `contract-process-topology` |
| `v-topology-single-instance` | `cargo test -p ma-engine --test topology second_instance_exits_without_mutation` | `contract-process-topology` |
| `v-topology-ui-kill` | `cargo test -p ma-engine --test topology ui_kill_keeps_recording` | `contract-process-topology` |
| `v-topology-update-deferred-during-session` | `cargo test -p ma-engine --test topology update_deferred_while_session_active` | `contract-process-topology` |

## Counts

23 T0, 69 T1 and 20 T2 verifications across
29 contracts. Every T2 identifier above must appear in `verification-tiers.toml`, and
`v-tier-every-t2-registered` fails the build if one does not.

## Gates

| Gate | When | Blocks |
| --- | --- | --- |
| portable continuous-integration job | every push and pull request | merge |
| windows continuous-integration job | every pull request into the default branch, and nightly | Phase 0 completion |
| `dev-docs conformance --root docs` | every push touching `docs/` | merge |
| ADR acceptance (proposed to accepted) | before unit 1 starts | the start of implementation |

## What would falsify this change

Phase 0 is not done, regardless of a green board, if any of the following holds: the negative fixture reports
a decoy or misses a violation; a T2 verification exists that is not in the tier file; the windows job has
never run green; an ADR remains at `proposed` while its contracts are being implemented; a required change
member is empty; or a check listed above has no corresponding test in the repository.
