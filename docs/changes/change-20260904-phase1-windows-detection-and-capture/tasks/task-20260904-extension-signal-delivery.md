---
id: task-20260904-extension-signal-delivery
kind: task
title: browser-extension-poc
status: done
created: '2026-09-04'
priority: normal
effort: medium
files_touched:
- extension/manifest.json
- extension/background.js
- extension/README.md
- crates/ma-ext-channel/tests/extension_poc.rs
pr: null
tags: []
owners: []
relations:
- {type: partOf, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
change: change-20260904-phase1-windows-detection-and-capture
summary: Build the detection-only manifest-v3 extension PoC that speaks the existing
  extension-channel wire contract using only the tabs API and the harness-provisioned
  endpoint file.
max_diff_loc: 300
pinned_context:
- docs/changes/change-20260904-phase1-windows-detection-and-capture/tasks/task-20260904-extension-signal-delivery.md
- docs/changes/change-20260904-phase1-windows-detection-and-capture/design-plan
updated: '2026-09-04'
---

## Responsibility

Build the detection-only manifest-v3 extension PoC that speaks the existing extension-channel wire contract using only the tabs API and the harness-provisioned endpoint file.

## Execution contract

- Output: An unpacked extension source tree under extension/ (manifest.json, background.js, README.md) with no framework and no content script, plus one Rust test in ma-ext-channel that reads the manifest.
- Tool guidance: Use chrome.tabs.query and chrome.tabs.onUpdated for host and audible only; never send a title, a URL path or a query string; do not add a permission beyond tabs and the loopback host; treat the generated endpoint file as untracked build output.
- Boundaries: Does not implement server-side validation, does not capture tab audio (an explicit PLAN non-goal), and does not change ExtensionMessage or the wire schema.

## Acceptance

- Given the extension loaded unpacked in Chrome with a Google Meet tab open, the background service worker reads the harness-written endpoint file from its own directory, posts host and audible for the active tab using only ExtensionMessage's existing fields, and receives 204.
- Given the manifest, permissions are exactly ["tabs"] plus host permission "http://127.0.0.1/*", there is no content_scripts key, no scripting, nativeMessaging, storage or <all_urls> permission, and no code path reads tab.url beyond its hostname; asserted mechanically by the manifest test.
- Given a 401 from the listener because the engine restarted and the token rotated, the worker stops posting and records the condition rather than retrying with a dead token.


{% transition from="todo" to="in_progress" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 8f3228ef0170 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}


{% transition from="in_progress" to="done" date="2026-09-04" %}
plan-delivery run plan-delivery-phase1-20260904-1: task commit 8f3228ef0170 on delivery/phase1-windows-detection-and-capture, mechanical gate approved
{% /transition %}
