---
id: adr-20261117-tab-recording-through-extension
kind: adr
title: Record browser meetings per tab through an extension
status: accepted
created: '2026-11-17'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- extension/manifest.json
- extension/background.js
- crates/app/src/browser/protocol.rs
- crates/app/src/platform/win/install.rs
summary: Browser meetings are recorded per tab by a bundled extension; automatic start
  relies on the undocumented --allowlisted-extension-id switch.
updated: '2026-11-17'
---

# Record browser meetings per tab through an extension

## Context

From outside the browser, a meeting in a tab can only be recorded as the browser window. The window shows whichever tab is selected, so switching tabs changes what is recorded, and the browser's audio is the audio of all tabs. Detection from outside is also indirect: the browser uses the microphone and a window title matches.

A browser extension can capture a single tab with `chrome.tabCapture` and can see the call in the page. But the browser allows tab capture only after the user invokes the extension on that tab, which contradicts automatic recording. The user asked whether tab recording could start automatically.

Options considered:

- Record the window. Works without setup; has the tab-switch and audio problems above.
- Require a click on the extension at the start of each meeting. Documented behavior; defeats the purpose of the product.
- Use `getDisplayMedia` from the extension. Shows a picker each time.
- Start the browser with `--allowlisted-extension-id=<id>`. This switch, intended for testing, exempts one extension from the user-gesture requirement.

The switch was verified on 2026-11-17 with a probe extension (`tools/tabcapture-probe`): capture without a gesture succeeded in Edge, and the user confirmed it in Chrome.

## Decision

Browser meetings are recorded per tab by a bundled Chrome/Edge extension. The extension detects a connected WebRTC call, and on the app's command records the tab and the microphone and sends the data to the app through a native-messaging relay.

Automatic start relies on `--allowlisted-extension-id`. The installer creates a browser launcher that adds the switch. The extension ID is fixed by a key in the manifest so that the switch and the native-host registration can name it.

Window recording remains the behavior when the extension is not connected.

## Consequences

- Positive: only the meeting tab and its audio are recorded, and recording continues while another tab is visible.
- Positive: a call is detected when it connects, including listen-only participation.
- Positive: the same mechanism is available on macOS, because the recording happens inside the browser.
- Negative: the switch is undocumented and may be removed. Tab recording then fails to start until another mechanism is found.
- Negative: setup has manual steps: loading the unpacked extension in developer mode, allowing the microphone, and always starting the browser from the launcher.
- Negative: a browser started with the switch shows no prompt when this extension captures a tab; the user accepts that at setup.
- Neutral: Firefox and other browsers without this extension keep window recording.

## Confirmation

`crates/app/src/bundle.rs` and `crates/app/src/install/mod.rs` test that the extension, the host name and the native-host manifest agree on the identifiers. That the switch still works in current browsers can only be confirmed by a manual run; the extension reports the failure in the log when it does not.


{% transition from="proposed" to="accepted" date="2026-11-17" %}
Decided by the user on 2026-11-17
{% /transition %}
