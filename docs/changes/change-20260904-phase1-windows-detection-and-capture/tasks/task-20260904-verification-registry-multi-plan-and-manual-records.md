---
id: task-20260904-verification-registry-multi-plan-and-manual-records
kind: task
title: verification-registry-multi-plan-and-manual-records
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- verification-tiers.toml
- manual-verification.toml
- xtask/src/verify.rs
- xtask/src/manual_record.rs
- xtask/src/main.rs
- xtask/tests/registration.rs
- docs/changes/change-20260904-phase1-windows-detection-and-capture/manual-verification/*.json
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
- {type: dependsOn, target: task-20260904-process-loopback-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Extend the verification registry to hold more than one canonical plan and
  to gate manual observations on a committed, non-stale, observation-complete record,
  then register every Phase 1 verification id and declare the per-application loopback-requirement
  comparison as one of those procedures.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-verification-registry-multi-plan-and-manual-records.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Extend the verification registry to hold more than one canonical plan and to gate manual observations on a committed, non-stale, observation-complete record, then register every Phase 1 verification id and declare the per-application loopback-requirement comparison as one of those procedures.

## Execution contract

- Output: A plans array in verification-tiers.toml with the single-plan form still accepted, a manual-verification.toml procedure table carrying required-observation keys, a manual_record subcommand in xtask, the registrations, and xtask tests.
- Tool guidance: Keep the existing single-plan field working so no other repository consumer breaks; keep records as plain JSON outside docs lint targets; do not weaken the existing exactly-once and tier-matching rules; read the adapter table ids from crates/ma-adapter-*/adapter.toml rather than naming any service in xtask source, which boundary.toml's literals rule confines to L4.
- Boundaries: Does not change the tier definitions themselves, does not add a CI job, does not perform any manual observation, and does not add a field to adapter.toml, AdapterSpec or any other shared contract.

## Acceptance

- Given verification-tiers.toml declaring plans = [Phase 0's spine, Phase 1's spine], cargo xtask verify --check-registration treats the union of their declared ids as the registered set, reports no stale registration for any Phase 0 id, and still rejects an id registered twice or absent.
- Given manual-verification.toml declaring one procedure per manual verification id with owner, host profile, steps, artifact path, pass criterion and the observation keys the record must carry, cargo xtask manual-record --id X --require pass exits non-zero when the record is missing, when its outcome is not pass, when it omits a declared required observation, or when its recorded procedure digest differs from the current procedure text.
- Given the procedure for v-win1-loopback-requirement-live-comparison, its required observation keys are exactly the adapter table ids discovered under crates/ma-adapter-*/adapter.toml, and a fixture record that omits one of them is rejected; the keys are read from the tables rather than written as literals, so no service identifier enters xtask.
- Given the xtask tests, every plan-declared id whose registered command is a manual-record invocation has a matching procedure entry, and every procedure entry names a plan-declared id.
- Given cargo xtask verify --check-registration, every Phase 1 T2 id is registered in the windows tier with platform = "windows" and every T0 and T1 id in the portable tier.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit a88393aacfbc on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit a88393aacfbc on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
