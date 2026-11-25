---
id: issue-20261117-detection-is-unverified-against-real-mee
kind: issue
title: Detection is unverified against real meeting services
status: open
created: '2026-11-17'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: note-20261125-windows-checks-of-the-handoff-2026-11-25}
subject_paths:
- crates/core/src/rules.rs
- extension/hook.js
summary: Slack huddles and Google Meet calls were detected in real use on one Mac
  on 2026-11-19 and on one Windows PC on 2026-11-25; Zoom, Teams, Webex and the other
  browser services have not been tried in a real call.
---

## Opportunity

Detection should be confirmed in real calls on the services the defaults claim to cover: Zoom, Microsoft Teams, Webex and Slack as desktop apps, and Google Meet, Teams, Zoom, Webex, Slack, Whereby and Jitsi Meet in the browser.

## Evidence

Verification on 2026-11-17 used a test process that held the microphone and a test page that held a WebRTC connection; see the [verification note](../note/note-20261117-local-verification.md). On 2026-11-19, on one Mac, Slack huddles and Google Meet calls in Chrome were detected and recorded, without other participants ([Slack huddle note](../note/note-20261119-slack-huddle-window-after-the-merge.md), [metadata note](../note/note-20261119-recording-metadata-verification.md)). The process names in `crates/core/src/rules.rs` for Zoom, Teams and Webex, and the URL patterns in `extension/manifest.json` other than Google Meet's, were written from knowledge of the services, not observed in calls. On 2026-11-25, on one Windows PC, a Slack huddle and a Google Meet call in Chrome were detected and recorded, without other participants ([Windows note](../note/note-20261125-windows-checks-of-the-handoff-2026-11-25.md)). Slack's process there was `Slack.exe`, and a child of it held the microphone.

## Context for follow-up

Questions a real call answers per service: the name of the process that holds the microphone; whether the app releases the microphone while muted, which would end the recording after `end_grace_seconds`; whether its call has a window of its own that opens after the microphone is taken, or one its titles tell apart from the main window, which would need an adapter in `crates/app/src/apps`; and whether `hook.js` sees a connected `RTCPeerConnection` in the page, including in pre-join screens where a false start is possible. `meeting-assistant --probe` prints the signals. The [meeting detection design](../design/design-meeting-detection.md) holds the contract; findings belong in a dated note, and changes to defaults in a change of their own.
