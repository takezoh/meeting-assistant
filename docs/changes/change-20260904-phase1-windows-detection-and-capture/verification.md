---
change: change-20260904-phase1-windows-detection-and-capture
role: verification
---

<!-- lifecycle is owned by change.md -->

# Verification

Phase 1 is complete when every success condition below has evidence from a named check that runs somewhere a
check can run. A check that is written but never run is a failing check — the rule
`contract-verification-tiering` established in Phase 0 — and Phase 1 adds the case that rule did not cover: an
observation no unattended runner can make. Those become declared procedures with committed, digest-pinned
records, gated by a command the hosted runner *can* execute.

Forty-six verifications: thirty-six on the portable tier (thirty-one T0 and five T1), ten on the windows tier
(T2), of which nine are manual-record gates and one is a real unattended Windows test.

## Success conditions

| # | condition | evidence |
| --- | --- | --- |
| S-1 | The nine PLAN section 6 Phase 1 exit criteria each have a check that fails when the criterion is violated | A-01 through A-09 green in their tiers |
| S-2 | A browser meeting is detected only when the tab and the microphone belong to the same process tree | `v-win1-same-tree-mic-corroborates`, `v-win1-cross-tree-mic-does-not-corroborate`, `v-win1-missing-tree-root-is-inconclusive`, with the existing `forged_extension_signal_does_not_start_capture` still green |
| S-3 | Meeting audio and microphone audio are both recordable, and a two-hour recording loses nothing | `v-win1-loopback-live-activation`, `v-win1-two-hour-chunk-accounting`, `v-win1-two-hour-live` |
| S-4 | Nothing Phase 1 observes enters the closed signal schema | `v-win1-no-new-signal-fields`, `v-win1-fixture-schema-conformance`, and the existing `v-signal-no-ui-text-fields` still green |
| S-5 | Recorded timelines replay as detector fixtures and explain their decisions without re-running the detector | `v-win1-fixture-replay-golden`, `v-win1-diagnostics-cite-signals`, `v-win1-harness-decisions-sidecar` |
| S-6 | Committed fixtures carry no real host, process or service identifier | `v-win1-fixture-redaction`, and `cargo xtask boundary` still green |
| S-7 | Phase 1 adds no automatic-start path | `v-win1-harness-requires-explicit-invocation`, with the existing `v-consent-no-surface-no-start` untouched and still green |
| S-8 | The endpoint descriptor's ACL is applied, and both trust-reversal observations are recorded | `v-win1-endpoint-descriptor-acl-applied`, `v-win1-endpoint-dacl-readability-observed`, `v-win1-browser-loopback-policy-observed` |
| S-9 | Every declared check is registered exactly once across both plans, and no Phase 0 registration became stale | `v-win1-registration-unions-plans`, `cargo xtask verify --check-registration` |
| S-10 | Every check the hosted runner cannot perform has a current, passing record that covers every observation its procedure declares | `v-win1-manual-procedures-declared`, `v-win1-manual-record-staleness`, `v-win1-loopback-requirement-record-shape`, and the nine manual-record gates |
| S-11 | The workspace still builds, tests and lints on a non-Windows host | `v-win1-portable-workspace-clippy`, `v-win1-windows-code-is-cfg-gated`, `cargo test --workspace` |
| S-12 | The two proposed decisions that change how an accepted ADR is realised are accepted before implementation | `governance.gate: hard` in `change.md`, with `approval_evidence` recorded at acceptance |

## Verification tiers

**portable** — `ubuntu-latest`, every push and pull request, blocks merge. Holds every Phase 1 T0 and T1, and
continues to run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` over the
whole workspace including the two Windows-only crates, which is why every native call site is gated with a
portable fake behind the same trait.

**windows** — a Windows 11 runner, pull requests into `main` and nightly. Holds every Phase 1 T2. One of the
ten is a real unattended test; the other nine are record gates, because the hosted image has no Teams, Slack,
Zoom or Chrome installation, no speaker and no microphone, and does not sit for two hours.

Registration is mandatory and unchanged in strength: an identifier absent from `verification-tiers.toml`, or
registered twice, or in the wrong tier for its plan tier, or without a `platform` marker in a platform-bound
tier, fails `cargo xtask verify --check-registration`. What changes is that the registry now names both
phases' plans and unions their declared identifiers, so no Phase 0 registration becomes stale.

### T0 — portable

| verification | command | contract |
| --- | --- | --- |
| `v-win1-process-identity-fixture` | `cargo test -p ma-signals-windows process_identity_from_fake_enumerator` | `contract-process-package-identity` |
| `v-win1-collector-restart-resync-fixture` | `cargo test -p ma-signals-windows restart_while_condition_true_sets_restart_resync` | `contract-process-package-identity` |
| `v-win1-mic-use-fixture` | `cargo test -p ma-signals-windows mic_use_from_fake_session_manager` | `contract-audio-session-mic-use` |
| `v-win1-mic-use-source-precedence` | `cargo test -p ma-signals-windows consent_store_never_emits_a_signal_alone` | `contract-audio-session-mic-use` |
| `v-win1-loopback-fallback-fixture` | `cargo test -p ma-capture process_loopback_falls_back_to_system_loopback_on_activation_failure` | `contract-process-loopback-capture` |
| `v-win1-manual-path-available` | `cargo test -p ma-capture manual_capture_source_available_independent_of_loopback_outcome` | `contract-process-loopback-capture` |
| `v-win1-capture-origin-rate-pinned` | `cargo test -p ma-capture wasapi_origin_is_pinned_to_sample_rate_and_mono` | `contract-process-loopback-capture` |
| `v-win1-mic-endpoint-fixture` | `cargo test -p ma-capture mic_endpoint_follows_supplied_session_endpoint` | `contract-mic-endpoint-follows-session` |
| `v-win1-mic-endpoint-successor-track` | `cargo test -p ma-capture endpoint_change_opens_successor_track` | `contract-mic-endpoint-follows-session` |
| `v-win1-leak-erl-fixture` | `cargo test -p ma-capture leak_erl_from_paired_fixture_tracks` | `contract-echo-leak-measurement` |
| `v-win1-leak-no-qualifying-window` | `cargo test -p ma-capture leak_measurement_reports_no_qualifying_window` | `contract-echo-leak-measurement` |
| `v-win1-loopback-requirement-record-shape` | `cargo test -p xtask loopback_requirement_record_covers_every_adapter_table` | `contract-per-app-loopback-requirement-record` |
| `v-win1-fixture-replay-golden` | `cargo test -p ma-detect windows_fixture_timelines_replay_byte_identical` | `contract-replayable-timeline-fixtures` |
| `v-win1-fixture-header-shape` | `cargo test -p ma-signal windows_fixture_header_matches_timeline_header_shape` | `contract-replayable-timeline-fixtures` |
| `v-win1-fixture-redaction` | `cargo test -p ma-signal windows_fixtures_carry_no_real_host_identifiers` | `contract-replayable-timeline-fixtures` |
| `v-win1-confirmation-label-sidecar-shape` | `cargo test -p ma-signal confirmation_label_matches_labels_json_shape` | `contract-replayable-timeline-fixtures` |
| `v-win1-harness-requires-explicit-invocation` | `cargo test -p ma-engine diagnostic_harness_requires_explicit_invocation` | `contract-diagnostic-session-harness` |
| `v-win1-harness-partial-timeline-survives-cancel` | `cargo test -p ma-engine cancelled_session_keeps_its_partial_timeline` | `contract-diagnostic-session-harness` |
| `v-win1-harness-label-command` | `cargo test -p ma-engine label_command_writes_labels_sidecar` | `contract-diagnostic-session-harness` |
| `v-win1-harness-decisions-sidecar` | `cargo test -p ma-engine session_end_writes_decisions_sidecar` | `contract-diagnostic-session-harness` |
| `v-win1-extension-message-shape` | `cargo test -p ma-ext-channel extension_poc_message_matches_existing_schema` | `contract-extension-signal-delivery` |
| `v-win1-extension-manifest-permissions` | `cargo test -p ma-ext-channel extension_manifest_declares_no_content_script_or_broad_host` | `contract-extension-signal-delivery` |
| `v-win1-diagnostics-cite-signals` | `cargo test -p ma-detect decision_cites_signal_ids_for_windows_fixtures` | `contract-detector-diagnostics-explainability` |
| `v-win1-capture-path-sources-cover-collectors` | `cargo test -p xtask capture_path_isolation_names_every_capture_path_crate` | `contract-capture-path-isolation-scope` |
| `v-win1-registration-unions-plans` | `cargo test -p xtask registration_unions_every_declared_plan` | `contract-windows-tier-verification-registration` |
| `v-win1-windows-code-is-cfg-gated` | `cargo test -p xtask windows_only_dependencies_are_target_gated` | `contract-windows-tier-verification-registration` |
| `v-win1-portable-workspace-clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | `contract-windows-tier-verification-registration` |
| `v-win1-manual-procedures-declared` | `cargo test -p xtask every_manual_verification_id_has_a_procedure` | `contract-manual-verification-record` |
| `v-win1-manual-record-staleness` | `cargo test -p xtask a_record_whose_procedure_changed_is_rejected` | `contract-manual-verification-record` |
| `v-win1-fixture-schema-conformance` | `cargo test -p ma-signal windows_fixtures_conform_to_signal_schema` | `contract-closed-schema-discipline` |
| `v-win1-no-new-signal-fields` | `cargo test -p ma-signal payload_and_subject_field_sets_are_unchanged` | `contract-closed-schema-discipline` |

### T1 — portable

| verification | command | contract |
| --- | --- | --- |
| `v-win1-two-hour-chunk-accounting` | `cargo test -p ma-capture two_hour_chunk_accounting_from_synthetic_source` | `contract-two-hour-durability` |
| `v-win1-same-tree-mic-corroborates` | `cargo test -p ma-detect same_process_tree_mic_and_tab_corroborate` | `contract-meet-corroboration-required` |
| `v-win1-cross-tree-mic-does-not-corroborate` | `cargo test -p ma-detect mic_use_from_a_different_process_tree_does_not_corroborate` | `contract-meet-corroboration-required` |
| `v-win1-missing-tree-root-is-inconclusive` | `cargo test -p ma-detect tab_without_a_process_tree_root_is_inconclusive` | `contract-meet-corroboration-required` |
| `v-win1-endpoint-descriptor-acl-applied` | `cargo test -p ma-ext-channel endpoint_write_applies_the_owner_only_descriptor` | `contract-extension-trust-reversal-check` |

### T2 — windows

One unattended test:

| verification | command | contract |
| --- | --- | --- |
| `v-win1-endpoint-dacl-readability-observed` | `cargo test -p ma-ext-channel --test trust_reversal endpoint_json_not_readable_by_other_same_user_process -- --ignored` | `contract-extension-trust-reversal-check` |

Nine record gates, each `cargo xtask manual-record --id <id> --require pass`:

| verification | observation the record must carry | contract |
| --- | --- | --- |
| `v-win1-process-identity-live-probe` | The four target applications' real image names and package family names as observed, and the synthetic mapping used in the fixtures | `contract-process-package-identity` |
| `v-win1-mic-use-latency-live` | The measured delay between a target application starting microphone capture and the session-manager-derived signal, per application | `contract-audio-session-mic-use` |
| `v-win1-loopback-live-activation` | Per application: whether process-loopback activation succeeded, the resulting capture mode and contamination risk, and whether both tracks consolidated | `contract-process-loopback-capture` |
| `v-win1-loopback-requirement-live-comparison` | Per application: the same meeting captured under single-process and under process-tree activation, and whether the second captured audio the first missed | `contract-per-app-loopback-requirement-record` |
| `v-win1-mic-endpoint-live` | The endpoint the meeting application used and the endpoint the recorded track opened, across at least one observed endpoint change | `contract-mic-endpoint-follows-session` |
| `v-win1-leak-live-per-app` | Per application on a speaker path: the echo return loss in dB with both levels, the window position and the alignment uncertainty, or the explicit non-measurement outcome | `contract-echo-leak-measurement` |
| `v-win1-two-hour-live` | The application, the wall-clock duration, the final manifest-versus-directory comparison and any gap records | `contract-two-hour-durability` |
| `v-win1-extension-live-chrome` | Whether the unpacked extension, provisioned by the harness, reached the listener and what the listener's counters showed | `contract-extension-signal-delivery` |
| `v-win1-browser-loopback-policy-observed` | Whether current Chrome and Edge policy permits a detection-only extension to reach a loopback listener, with the policy state observed | `contract-extension-trust-reversal-check` |

## The manual-record gate

`manual-verification.toml` declares, per identifier: `owner`, `host_profile`, ordered `steps`, `artifact`,
`pass_criterion`, `required_observations`, and the digest of the procedure text. A performed observation is a
committed JSON record under `manual-verification/` naming the identifier, `performed_at`, `performed_by`, the
host profile, the outcome (`pass`, `fail` or `blocked`), the observations and the `procedure_digest` it was
performed against.

`cargo xtask manual-record --id <id> --require pass` fails when the record is absent, when the outcome is not
`pass`, when the observations omit a key the procedure declares as required, or when the recorded digest
differs from the current procedure text. Editing a procedure therefore invalidates every record taken against
the old text, which is what stops a changed procedure from silently inheriting an old result; and a record
cannot claim `pass` while leaving most of its subject unobserved, which is what makes
`v-win1-loopback-requirement-live-comparison` a per-application record rather than a per-application-if-someone-
bothered one. For that identifier the required keys are one per adapter table, read from
`crates/ma-adapter-*/adapter.toml` rather than written as literals, because `boundary.toml` confines service
identifiers to L4 crates and `xtask` is L5.

What the gate does **not** prove: that the person performed the procedure honestly, or that a value they wrote
is the value they measured. It proves presence, coverage, outcome and freshness. That is a real limit and it is
the reason the nine are exactly the observations no machine on this project can make, and not one more.

## Gates

- The portable job blocks merge on every push and pull request: `cargo xtask boundary`, `cargo deny`,
  `cargo xtask verify --check-registration`, `cargo xtask verify --tier portable --strict`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` and
  the out-of-workspace UI client test. No new job or step is added.
- The windows job is the Phase 1 exit gate, as it was for Phase 0. It runs
  `cargo xtask verify --tier windows --strict`, which now includes the nine record gates. Phase 1 is not
  complete until it reports green, which means until nine records exist, pass, and match their procedures.
- `cargo xtask docs-check` continues to enforce ADR placement and tripolar consequences, the fixed design-document
  set, the Phase 0 change members, promotion-none and frontmatter schema conformance across `docs/`.
- `governance.gate: hard` in `change.md` blocks implementation until the eight proposed ADRs are accepted;
  acceptance is recorded in `approval_evidence` and in each ADR's own transition.

## What would falsify this change

- A conforming implementation in which a Google Meet tab in one browser process tree is corroborated by
  microphone use in another. `v-win1-cross-tree-mic-does-not-corroborate` is the check; if it can be made to
  pass while the behaviour persists, the join is in the wrong place.
- A conforming implementation whose committed decision identifiers differ from another's for the same fixture.
  `v-win1-fixture-replay-golden` and the existing `replay_is_byte_identical` are the checks; a divergence means
  either the table version moved when it should not have, or the join introduced non-determinism.
- A 48 kHz WASAPI device that reaches the chunk writer, making `CHUNK_SAMPLES` mean ten seconds and the loss
  window twenty. `v-win1-capture-origin-rate-pinned` is the check; the draft's manifest-versus-directory and
  no-data-loss checks both pass in that state, which is why this one exists.
- Two per-application echo numbers that are not comparable because they were computed by different methods.
  `v-win1-leak-erl-fixture` pins the statistic to a synthesised known value; if two implementations can both
  pass it and still disagree on a real recording, the method is under-specified.
- A Phase 0 verification identifier reported as a stale registration after Phase 1's plan lands.
  `v-win1-registration-unions-plans` and `cargo xtask verify --check-registration` are the checks.
- A `ma-capture` to `ma-signals-windows` edge, or a service identifier in `ma-engine` source.
  `cargo xtask boundary` is the check for both, and the second is why every adapter dependency is renamed.
- A green windows job with no manual records present. `v-win1-manual-procedures-declared` and the nine record
  gates are the checks; if the job can go green without them, the split between what CI can observe and what a
  person must observe has been erased rather than made explicit.
- A passing loopback-requirement record that names fewer applications than there are adapter tables, so FR-107
  is met for one target and silently unmet for three. `v-win1-loopback-requirement-record-shape` and the gate's
  required-observation rejection are the checks; without them, folding the fact out of the adapter table would
  have left it with presence but no coverage.
