---
id: task-20260904-extension-trust-reversal-check
kind: task
title: extension-endpoint-acl-and-trust-checks
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- crates/ma-ext-channel/src/auth.rs
- crates/ma-ext-channel/src/server.rs
- crates/ma-ext-channel/tests/trust_reversal.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Apply the endpoint descriptor's ACL that Phase 0 only built, carry the peer
  process tree root into tab signals, and record the two trust-reversal observations
  adr-20260903-extension-localhost-channel-trust assigns to Phase 1 without pre-judging
  their outcome.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-extension-trust-reversal-check.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Apply the endpoint descriptor's ACL that Phase 0 only built, carry the peer process tree root into tab signals, and record the two trust-reversal observations adr-20260903-extension-localhost-channel-trust assigns to Phase 1 without pre-judging their outcome.

## Execution contract

- Output: An injected ACL applier and its Windows implementation in auth.rs, an additive Request field copied in server.rs, one integration test file, and the manual procedure entry for the browser-policy observation.
- Tool guidance: Do not change the descriptor's shape, Authenticator.check, the token lifetime or the wire schema; an observation that violates the intended trust model is recorded and raised as an open decision, not worked around.
- Boundaries: Does not decide whether to supersede the ADR, does not add a bootstrap or discovery endpoint, and does not change the extension's provisioning mechanism.

## Acceptance

- Given EndpointDescriptor::write, the owner-only SecurityDescriptor it already builds is applied to endpoint.json through an injected applier before the path is returned; a fake applier in the portable test records the call and the descriptor, and on Windows the applier sets the file's DACL from the descriptor's SDDL.
- Given a live Windows run, whether endpoint.json is readable by another same-user process is recorded as a pass or a fail, never skipped; given a live Chrome or Edge run under current policy, whether the detection-only extension can reach the loopback listener is recorded in the manual record, never skipped.
- Given Request, an additive peer_process_tree_root_pid field supplied by the transport is copied by signals_for into Payload.process_tree_root_pid on every emitted tab signal; ExtensionMessage, Authenticator.check, the token generation, the rate, freshness and queue limits and the rejection status table are unchanged.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 139cc6d9a829 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 139cc6d9a829 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
