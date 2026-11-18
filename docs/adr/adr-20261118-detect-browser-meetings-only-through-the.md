---
id: adr-20261118-detect-browser-meetings-only-through-the
kind: adr
title: Detect browser meetings only through the extension
status: accepted
created: '2026-11-18'
decision_makers:
- user
tags: []
owners: []
relations:
- {type: references, target: adr-20261117-tab-recording-through-extension}
source_paths:
- crates/core/src/rules.rs
- crates/core/src/detector.rs
summary: The window-title rule for browsers is removed; browser meetings are detected
  and recorded only through the extension, on Windows and macOS alike.
updated: '2026-11-18'
---

# Detect browser meetings only through the extension

## Context

Browser meetings had two paths. With the extension, a connected call is reported from the page and its tab is recorded. Without it, a default rule named the browsers and required a window title matching one of a list of patterns while the browser used the microphone; the browser window was then recorded, which follows whatever tab is visible and includes every tab's sound. Rules carried `require_window_title` and `window_title_patterns` only for this. The user said on 2026-11-18 that browsers without the extension need not be supported, on any platform.

## Decision

Browser meetings are detected and recorded only through the extension. The `WebMeeting` default rule, `require_window_title` and `window_title_patterns` are removed, and a configuration that still contains those keys is rejected at startup. A rule's condition is now a single one, its app using the microphone, which both starts and continues the meeting; the recorded window is the app's largest. A browser connected to the extension stays excluded from rules, in case a user writes a rule naming it.

## Consequences

- Positive: a browser that uses the microphone for something else is never taken for a meeting, and no recording follows the visible tab of a browser window.
- Positive: rules and the detector are simpler: one condition, no regular expressions, no title-dependent window choice.
- Negative: a browser meeting is not recorded automatically without the extension, including in browsers the extension does not support.
- Neutral: macOS needs no window-title detection when native capture is added.

## Confirmation

`crates/core/tests/rules.rs` asserts that a browser using the microphone with a meeting title is not active under the default rules, and `crates/core/tests/config.rs` that the default rules name no browser and that `require_window_title` is rejected.
