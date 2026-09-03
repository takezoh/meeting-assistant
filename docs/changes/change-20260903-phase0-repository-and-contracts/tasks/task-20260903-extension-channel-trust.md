---
id: task-20260903-extension-channel-trust
kind: task
title: extension-channel-contract
status: done
created: '2026-09-03'
priority: normal
effort: medium
files_touched:
- crates/ma-ext-channel/Cargo.toml
- crates/ma-ext-channel/src/server.rs
- crates/ma-ext-channel/src/auth.rs
- crates/ma-ext-channel/src/message.rs
- contracts/extension-channel/message.schema.json
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260903-phase0-repository-and-contracts}
- {type: dependsOn, target: task-20260903-signal-and-detector-contracts}
source_paths: []
change: change-20260903-phase0-repository-and-contracts
summary: Fix the detection-only browser channel's message schema, authentication and
  non-authoritative status.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260903-phase0-repository-and-contracts/tasks/task-20260903-extension-channel-trust.md
- docs/changes/change-20260903-phase0-repository-and-contracts/design-plan
updated: '2026-09-03'
---

## Responsibility

Fix the detection-only browser channel's message schema, authentication and non-authoritative status.

## Execution contract

- Output: A Rust crate with an injected transport, plus a JSON Schema for the message.
- Tool guidance: Make the transport injectable so authentication and rejection paths are testable without a browser; record the native-messaging alternative in the ADR before committing to loopback.
- Boundaries: Do not build the browser extension itself; do not accept any audio or DOM content over this channel.

## Acceptance

- a request without the token, with a web origin, or with a stale sequence is rejected and produces no signal
- the endpoint descriptor file is created with an owner-only ACL and the token is regenerated per engine start
- accepted messages become signals carrying host and tab key only, never a full URL or title
- a forged extension signal without a corroborating microphone signal does not start capture


{% transition from="todo" to="in_progress" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit a196212e501e with approved mechanical gate
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-03" %}
plan-delivery run plan-delivery-phase0-20260903-2: task commit a196212e501e with approved mechanical gate
{% /transition %}
