---
id: issue-20261119-the-extension-s-javascript-has-no-automa
kind: issue
title: The extension's JavaScript has no automated tests
status: done
created: '2026-11-19'
issue_type: plan
tags: []
owners: []
relations: []
subject_paths:
- extension/background.js
- extension/offscreen.js
- extension/hook.js
summary: The extension's logic, such as meetingTitle, is checked only by hand in a
  browser; a test runner for its pure functions was proposed and not started.
disposition_reason: Extension tests in extension/test run background.js and offscreen.js
  with fake browser APIs through node --test; see docs/technical/testing.md.
updated: '2026-11-19'
---

## Opportunity

The extension's pure functions, such as `meetingTitle` in `background.js`, could be tested automatically, for example with Node's built-in test runner. Today the extension is checked only by hand in a browser.

## Evidence

`cargo test` covers the app's side of the link (`browser/{protocol,hub,server}.rs`) and that the bundled files carry the fixed identifiers (`bundle.rs`), not the extension's own logic. The proposal came from the structure review recorded in the macOS handoff of 2026-11-18, which was removed once its work was done.

## Context for follow-up

The extension is plain JavaScript loaded by the browser without a build step; see [the browser tab recording design](../design/design-browser-tab-recording.md). Node is installed through mise on the development Mac but has no default version set.


{% transition from="open" to="done" date="2026-11-19" %}
Tests added with Node's own test runner.
{% /transition %}
