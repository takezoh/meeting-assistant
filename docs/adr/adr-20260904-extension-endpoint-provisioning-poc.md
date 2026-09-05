---
id: adr-20260904-extension-endpoint-provisioning-poc
kind: adr
title: The Phase 1 extension is provisioned by the diagnostic harness
summary: The unpacked PoC extension learns the loopback port and per-start token from
  a file the diagnostic harness writes into its directory, because an MV3 service
  worker limited to the tabs API can read neither the descriptor nor the port.
status: accepted
created: '2026-09-04'
decision_makers:
- take
consequences:
  positive:
  - The extension can actually reach the listener, which the drafted design could
    not, without adding a permission, a listener, a discovery endpoint or a response
    body.
  - Every rule adr-20260903-extension-localhost-channel-trust fixes stays intact -
    the token still rotates per engine start and every message still needs both the
    token and the pinned extension origin.
  - Provisioning stays entirely inside the diagnostic harness, so nothing about it
    survives into a shipped product by accident.
  negative:
  - The mechanism works only for an unpacked, developer-loaded extension, so the provisioning
    question for a store-installed extension is deferred rather than answered.
  - An engine restart rotates the token while the loaded worker holds the old one,
    so the operator must reload the extension after restarting the harness.
  - The generated endpoint file puts a live token inside the extension directory,
    which is protected only by the user profile's own permissions rather than by the
    owner-only ACL that guards endpoint.json.
  neutral:
  - The generated file is untracked build output and is named in the repository ignore
    rules rather than committed.
  - The two Windows-tier trust observations NFR-103 requires are unaffected, because
    they concern the descriptor's ACL and the browser's loopback policy, not provisioning.
confirmation: cargo test -p ma-ext-channel extension_manifest_declares_no_content_script_or_broad_host
  (T0) and extension_poc_message_matches_existing_schema (T0); cargo xtask manual-record
  --id v-win1-extension-live-chrome --require pass (T2).
tags:
- browser
- extension
- detection
- security
owners:
- take
relations:
- {type: originatedFrom, target: change-20260904-phase1-windows-detection-and-capture}
- {type: references, target: adr-20260903-extension-localhost-channel-trust}
source_paths:
- crates/ma-ext-channel/src/auth.rs
- crates/ma-ext-channel/src/server.rs
- PLAN.md
updated: '2026-09-05'
---

## Context

`adr-20260903-extension-localhost-channel-trust` fixes the channel: the engine binds `127.0.0.1` on an
**ephemeral** port, writes an endpoint descriptor holding the port and a 256-bit **per-start** token to
`%LOCALAPPDATA%\MeetingAssistant\ext\endpoint.json` with an owner-only ACL, and `Authenticator::check` requires
both that token and an origin equal to `chrome-extension://<pinned id>` on every request.

PLAN section 4 constrains the extension to detection only, and the Phase 1 plan constrains its manifest to the
`tabs` permission plus the loopback host, with no content script and no DOM access.

Those two constraints are jointly unsatisfiable as drafted. A manifest-v3 service worker cannot read a file
anywhere on disk, so it learns neither the ephemeral port nor the token; and without them every request it
makes is rejected. The design critique recorded this as a blocker: `FR-110` and acceptance criterion `A-02`
asserted the reporting "works" while naming no acquisition path, so every implementer would invent a different
one.

Phase 1 is explicitly a PoC — PLAN calls it a "detection-only browser extension PoC" — and it has no installer,
no store listing and no enterprise policy deployment.

## Decision

The Phase 1 extension is **loaded unpacked and provisioned by the diagnostic harness**.

`ma-diag` is given the path of the unpacked extension directory. At engine start it writes the current
listener port and token into a generated file in that directory, which the service worker imports at startup
before it posts anything. Nothing else changes: the listener still binds an ephemeral port, `endpoint.json` is
still written with its owner-only descriptor (now actually applied, see `NFR-103`), the token still rotates on
every engine start, and every message still presents both the token and the pinned extension origin. The
generated file is untracked build output.

When the engine restarts and the loaded worker still holds the previous token, the listener returns 401. The
worker stops posting and records the condition rather than retrying with a dead token; the operator reloads the
extension.

Provisioning for a **store-installed** extension is explicitly not decided here. It belongs to the phase that
builds the installer, and the three alternatives below are the candidates it will choose among.

## Alternatives considered

**An origin-pinned bootstrap endpoint.** The listener would answer an unauthenticated `GET` whose `Origin` is
the pinned `chrome-extension://<id>` with the current token, and the extension would find the port by probing a
fixed range. Attractive because it generalises to a store-installed extension. Rejected for Phase 1 on three
counts. It hands the token to anything that can set an `Origin` header, which a local same-user process can,
so it decides in advance that the token has no value against a same-user process — precisely the question
`NFR-103`(a) is measuring in this same phase, and deciding it before the measurement discards the measurement's
purpose. It requires `Response`, whose entire contract today is a status code with no body, to grow a body
path. And it adds a second, weaker authentication path to a channel whose current strength is that there is
only one.

**Native messaging now.** Materially stronger: the browser launches the host over stdio and authenticates by
extension identifier from a registry-registered manifest, removing the port, the token file and the hostile-page
surface entirely. Rejected because it is the accepted ADR's own named reversal target, gated on two Phase 1
observations that do not yet exist; adopting it now would discard both the evidence the ADR asks Phase 1 to
collect and the already-tested loopback suite, on a guess about what that evidence will say.

**Installer or `chrome.storage.managed` provisioning.** The desktop installer writes an enterprise policy
naming a deterministic port and a long-lived per-installation secret. Rejected because Phase 1 has no installer,
and because a long-lived secret would have to outlive the per-start token, contradicting the accepted ADR's
"the token is regenerated on every engine start, so a leaked token dies with the process".

**Dropping the token for extension traffic and authenticating on the pinned origin alone.** Smaller than a
bootstrap and equivalent to it in strength, since a bootstrap hands the token to anyone who can set the header
anyway. Rejected because it changes `Authenticator::check`, which the accepted ADR fixes normatively, and
because it would delete the `request_without_token_rejected` verification that ADR names in its confirmation.

## Consequences

**Positive.**

- The extension can actually reach the listener — the drafted design could not — without a new permission, a
  new listener, a discovery endpoint or a response body.
- Every rule the accepted channel ADR fixes stays intact: per-start token, pinned origin, both required.
- The mechanism lives entirely inside the diagnostic harness, so none of it survives into a shipped product by
  accident.

**Negative.**

- It works only for an unpacked, developer-loaded extension. The provisioning question for a store-installed
  extension is deferred, not answered, and a later phase must answer it.
- An engine restart rotates the token while the loaded worker holds the old one, so the operator has to reload
  the extension after restarting the harness. The 401 makes this visible rather than silent, but it is manual.
- The generated file puts a live token inside the extension directory under the browser profile's own
  permissions, not under the owner-only ACL that guards `endpoint.json`. On a single-user diagnostic machine
  that is the same trust domain, but it is a weaker guard than the descriptor's, and it is why this mechanism
  is scoped to the PoC.

**Neutral.**

- The generated file is untracked build output named in the repository ignore rules, not committed state.
- The two Windows-tier trust observations `NFR-103` requires are unaffected: they concern the descriptor's
  applied ACL and the browser's loopback policy, neither of which provisioning touches.

## Confirmation

`cargo test -p ma-ext-channel extension_manifest_declares_no_content_script_or_broad_host` (T0), which reads
`extension/manifest.json` and fails on any permission beyond `tabs` and the loopback host;
`extension_poc_message_matches_existing_schema` (T0); `cargo xtask manual-record --id
v-win1-extension-live-chrome --require pass` (T2, windows tier), whose record states whether a real Chrome
session reached the listener.


{% transition from="proposed" to="accepted" date="2026-09-04" %}
consultation-phase1-20260904-1 (2026-09-04): accepted by the conductor under the user's delegated authority for technical dispositions
{% /transition %}
