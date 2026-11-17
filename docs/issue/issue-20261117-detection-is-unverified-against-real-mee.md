---
id: issue-20261117-detection-is-unverified-against-real-mee
kind: issue
title: Detection is unverified against real meeting services
status: open
created: '2026-11-17'
issue_type: plan
tags: []
owners: []
relations: []
subject_paths:
- crates/core/src/rules.rs
- extension/hook.js
summary: The default rules and the in-page call detection were exercised with synthetic meetings only, not in real calls on each service.
---

## Opportunity

Detection should be confirmed in real calls on the services the defaults claim to cover: Zoom, Microsoft Teams, Webex and Slack as desktop apps, and Google Meet, Teams, Zoom, Webex, Slack, Whereby and Jitsi Meet in the browser.

## Evidence

Verification on 2026-11-17 used a test process that held the microphone and a test page that held a WebRTC connection; see the [verification note](../note/note-20261117-local-verification.md). The process names and title patterns in `crates/core/src/rules.rs` and the URL patterns in `extension/manifest.json` were written from knowledge of the services, not observed in calls.

## Context for follow-up

Questions a real call answers per service: the name of the process that holds the microphone; whether the app releases the microphone while muted, which would end the recording after `end_grace_seconds`; the window title during a call; which window the app's largest-window heuristic selects; and whether `hook.js` sees a connected `RTCPeerConnection` in the page, including in pre-join screens where a false start is possible. `meeting-assistant --probe` prints the signals. The [meeting detection design](../design/design-meeting-detection.md) holds the contract; findings belong in a dated note, and changes to defaults in a change of their own.
