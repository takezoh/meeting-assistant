---
id: adr-20260903-update-and-manifest-distribution
kind: adr
title: Static distribution over GitHub Releases with client-verified signed manifests
summary: Installers and manifests are hosted statically, update and adapter manifests
  are Ed25519-signed and verified before any declared value is used, with rollback
  protection and key rotation.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - No first-party service exists on any path, including updates.
  - A compromised or substituted host cannot cause an installation, because the signature
    is checked on the client.
  - A signed but stale release cannot be replayed onto a newer installation.
  negative:
  - Signing keys become a long-lived operational responsibility, and losing the current
    key without a rollover breaks updates for everyone.
  - There is no staged rollout and no remote kill switch, so a bad release is only
    mitigated by publishing a newer one.
  - GitHub Releases availability affects the update path, though never the recording
    path.
  neutral:
  - Adapter manifests use the same trust machinery as update manifests, so there is
    one verification path rather than two.
  - Deferring engine replacement during a session means an update can be pending for
    the length of a meeting.
confirmation: cargo test -p ma-manifest tampered_manifest_rejected (T1), replayed_older_manifest_rejected
  (T1), unknown_key_rejected_rollover_accepted (T1), digest_mismatch_blocks_adapter_activation
  (T1); cargo test -p ma-engine --test topology update_deferred_while_session_active
  (T2).
tags:
- distribution
- security
- supply-chain
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes static hosting on GitHub Releases, a code-signed installer, and Ed25519-signed update and adapter manifests verified by the Tauri updater and by the application before adapter activation, with no backend service. PLAN section 7 requires that no first-party backend appear on any path.

What the decision does not state is the ordering rule that makes signature verification actually protective, and what happens to a manifest that verifies but is old.

## Decision

Update and adapter manifests are Ed25519-signed and **verified before any declared value is used** — including before logging one. Verification happens on bytes, and parsing happens after verification succeeds, so an unverified document can never influence control flow.

A manifest is rejected unless its declared version is **strictly greater** than the installed version, which closes the rollback path where an attacker replays a genuinely signed older release with a known vulnerability. A declared artifact digest that does not match the file on disk blocks activation.

Key rotation works by a rollover block signed by the **current** key that introduces the next key; a manifest signed only by an unknown key is refused. All of this is a client-side decision: no server is trusted to decide what the client should install, which is what keeps the no-backend property true for the update path as well as the workflow path.

An engine replacement is deferred while any session is non-terminal, so an update never interrupts a recording.

## Alternatives considered

**A thin first-party relay for update metadata and token exchange.** Would allow staged rollout, kill switches and cleaner OAuth. Rejected because it puts a service the user does not control on a path the product promises not to have, and because it becomes an availability dependency for a local-first application.

**Transport security alone (HTTPS without signatures).** Rejected because it trusts the hosting provider and any future mirror, and because a signature is verifiable offline and after the fact.

**Version equality rather than strict increase.** Rejected because it permits replaying an older signed release.

**Trusting a server-provided key list.** Rejected because it moves the trust anchor to the thing being authenticated.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- No first-party service exists on any path, including updates.
- A compromised or substituted host cannot cause an installation, because the signature is checked on the client.
- A signed but stale release cannot be replayed onto a newer installation.

**Negative.**

- Signing keys become a long-lived operational responsibility, and losing the current key without a rollover breaks updates for everyone.
- There is no staged rollout and no remote kill switch, so a bad release is only mitigated by publishing a newer one.
- GitHub Releases availability affects the update path, though never the recording path.

**Neutral.**

- Adapter manifests use the same trust machinery as update manifests, so there is one verification path rather than two.
- Deferring engine replacement during a session means an update can be pending for the length of a meeting.

## Confirmation

cargo test -p ma-manifest tampered_manifest_rejected (T1), replayed_older_manifest_rejected (T1), unknown_key_rejected_rollover_accepted (T1), digest_mismatch_blocks_adapter_activation (T1); cargo test -p ma-engine --test topology update_deferred_while_session_active (T2).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
