---
id: adr-20260903-extension-localhost-channel-trust
kind: adr
title: Detection-only browser channel over an authenticated loopback listener
summary: The browser extension reports tab signals over a token-authenticated loopback
  listener with a pinned origin, and such signals can never alone start a recording.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Browser meetings are detectable without any DOM access.
  - A hostile web page cannot read the token file, and even a fully compromised channel
    can produce at most a spurious candidate.
  - The channel is testable without a browser because the transport is injected.
  negative:
  - A locally reachable listening port exists for the life of the engine, which is
    a second local endpoint beyond the named pipe and must be defended and audited.
  - Users must install an extension for browser detection to work at all, and without
    it browser meetings are manual.
  - The corroboration requirement means a genuinely audible browser meeting with no
    microphone use will not start automatically.
  neutral:
  - The token rotates per engine start, so an extension must re-read the descriptor
    after an engine restart.
  - Native messaging remains a live alternative with a named trigger rather than a
    dropped idea.
confirmation: cargo test -p ma-ext-channel request_without_token_rejected (T1), web_origin_rejected
  (T1), stale_sequence_rejected (T1); cargo test -p ma-detect forged_extension_signal_does_not_start_capture
  (T1).
tags:
- security
- detection
- browser
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

Browser meetings cannot be detected from operating-system signals alone with enough precision, so the user decision accepts a detection-only browser extension. Any local endpoint the extension can reach is also reachable, in principle, by a hostile web page running in the same browser, so the channel needs an authentication story and a bound on what a compromised channel can achieve.

## Decision

The engine binds `127.0.0.1` on an ephemeral port with exclusive address use and writes an endpoint descriptor containing the port and a 256-bit token to a file with an owner-only access control list. Every request must present the token **and** an origin matching the pinned extension identifier; any request with an `http:` or `https:` origin, or without the token, is rejected with no body and counted. Messages carry a strictly increasing per-instance sequence and a **5-second freshness window**; the channel accepts at most **20 messages per second** and queues at most **200**, dropping oldest-first and counting the drops. Accepted messages become signals carrying only host, tab key, audible and meeting-present — never a full URL, never a page title. The token is regenerated on every engine start, so a leaked token dies with the process.

**Extension signals are non-authoritative.** They may raise a candidate and may contribute to an end decision, but a determinate start additionally requires an operating-system microphone signal whose subject process belongs to the same browser process tree. This is simultaneously the security property (a forged tab signal cannot cause a recording) and the robustness property PLAN section 4 asks for.

The reversal condition is recorded so it is checked rather than remembered: if Phase 1 finds the token file readable by a same-user process the extension trust model must exclude, or if browser policy restricts extension access to loopback, the transport moves to native messaging and this ADR is superseded.

## Alternatives considered

**Chrome and Edge native messaging.** Materially stronger: the browser launches the host process over stdio and authenticates by extension identifier from a registry-registered manifest, which removes the listening port, the token file and the entire hostile-web-page attack surface. Not taken for Phase 0 because it costs per-browser registry registration inside the installer, a host process per browser, and a forwarding hop into the engine — and because the residual risk it removes is already bounded to a spurious candidate by the non-authoritativeness rule. This is the alternative most likely to be adopted later, and the message schema, the non-authoritativeness rule and all but two verifications survive the move unchanged.

**A fixed well-known port.** Rejected: trivially discoverable and collides with other software.

**No authentication, relying on the corroboration rule alone.** Rejected because an open unauthenticated local port is an invitation independent of what it can achieve, and because the counter would then measure noise rather than attacks.

**Treating extension signals as authoritative.** Rejected: a forged tab signal would then start a recording, and browser extension supply chains are not a trust anchor for that.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Browser meetings are detectable without any DOM access.
- A hostile web page cannot read the token file, and even a fully compromised channel can produce at most a spurious candidate.
- The channel is testable without a browser because the transport is injected.

**Negative.**

- A locally reachable listening port exists for the life of the engine, which is a second local endpoint beyond the named pipe and must be defended and audited.
- Users must install an extension for browser detection to work at all, and without it browser meetings are manual.
- The corroboration requirement means a genuinely audible browser meeting with no microphone use will not start automatically.

**Neutral.**

- The token rotates per engine start, so an extension must re-read the descriptor after an engine restart.
- Native messaging remains a live alternative with a named trigger rather than a dropped idea.

## Confirmation

cargo test -p ma-ext-channel request_without_token_rejected (T1), web_origin_rejected (T1), stale_sequence_rejected (T1); cargo test -p ma-detect forged_extension_signal_does_not_start_capture (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
