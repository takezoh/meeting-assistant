---
id: design-threat-model
kind: design
title: "Threat model and trust boundaries"
summary: Names every trust boundary of the meeting assistant, what crosses it, the accepted residual risks, and the checks that keep the boundaries mechanical.
status: active
created: '2026-09-03'
scope_type: policy
responsibilities:
  - id: RESP-001
    statement: "Enumerate every trust boundary the product has and what may cross each one."
  - id: RESP-002
    statement: "Record accepted residual risks with the reason they are accepted."
  - id: RESP-003
    statement: "Bind each boundary to the contract and the verification that enforce it."
invariants:
  - id: INV-001
    statement: "No secret value is serialized by a general-purpose serializer or written to any file the application writes (v-credential-type-not-displayable, v-credential-no-secret-in-any-written-file)."
    enforcement: test
  - id: INV-002
    statement: "The diagnostic bundle of any run contains no meeting content and no secret (v-redaction-content-type-not-loggable, v-redaction-marker-scan)."
    enforcement: test
  - id: INV-003
    statement: "The engine control pipe never exists with a permissive DACL, even transiently (v-authz-dacl-shape)."
    enforcement: test
  - id: INV-004
    statement: "Every host the product may contact is declared in egress-inventory.toml under a user_account, distribution or operating_system owner (v-egress-inventory-complete, v-egress-inventory-no-first-party)."
    enforcement: test
boundaries:
  provides:
    - the list of trust boundaries and residual risks that every security review starts from
  consumes:
    - contract-credential-custody
    - contract-diagnostic-redaction
    - contract-ipc-transport-authz
    - contract-egress-inventory
    - contract-extension-channel-trust
    - contract-release-manifest-trust
  forbidden:
    - a first-party backend in any data path
    - an anonymous fallback when a credential is missing
    - deleting a user's remote objects on the user's behalf
variability:
  fixed:
    - the three integration owners
    - the owner-only descriptor shape
    - the Secret and Content types as the only carriers of secrets and meeting text
  free:
    - the zeroization mechanism inside Secret
    - the notification platform used for the consent surface
capabilities:
  - id: cap:secret-custody
    uniqueness: global
  - id: cap:log-redaction
    uniqueness: global
  - id: cap:transport-authorization
    uniqueness: global
  - id: cap:egress-containment
    uniqueness: global
failure_responsibilities:
  - id: RESP-001
    statement: "Enumerate every trust boundary the product has and what may cross each one."
  - id: RESP-002
    statement: "Record accepted residual risks with the reason they are accepted."
  - id: RESP-003
    statement: "Bind each boundary to the contract and the verification that enforce it."
trust_boundaries:
  - id: TB-001
    statement: "operating system to engine: process, package-identity, audio-session and microphone facts cross in a closed signal envelope with no free-text field."
  - id: TB-002
    statement: "browser extension to engine: non-authoritative tab signals cross the loopback channel under a token-authenticated listener with pinned origin and freshness window; extension evidence never starts a recording alone."
  - id: TB-003
    statement: "interface host to engine: JSON-RPC commands and events cross the named pipe under an owner-only DACL, FILE_FLAG_FIRST_PIPE_INSTANCE, client SID comparison and per-channel server authenticity."
  - id: TB-004
    statement: "engine to processor host child: staged input files, a manifest-templated argument vector and secrets via the environment block or stdin cross to a per-job child under a job object; never argv."
  - id: TB-005
    statement: "product to the user's own accounts: exported transcripts, summaries and audio cross only to hosts in the egress inventory, with a per-send egress_audit row and credentials read on demand."
  - id: TB-006
    statement: "distribution to product: installers and update or adapter manifests cross only when code-signed or Ed25519-signed and verified before any declared value is used."
compatibility_policies:
  - Adding a host requires an inventory entry with a closed owner; removing the last reference to an active host requires removing the entry.
  - Adding a secret requires a Secret<T>; adding meeting text to a type requires Content.
tags: [security, threat-model]
owners: [take]
relations:
  - type: originatedFrom
    target: change-20260903-phase0-repository-and-contracts
source_paths:
  - crates/ma-secure/src/redaction.rs
  - crates/ma-secure/src/acl.rs
  - egress-inventory.toml
---

## Purpose

This document names every trust boundary of the meeting assistant, what is allowed to cross it, and the
mechanical check that keeps the boundary honest. It exists so that a security review starts from a list
rather than from a reading of the code, and so that the two Phase 0 exit criteria that concern
trust — no proprietary backend in the data path, and no leak of secrets or meeting content — have a
falsifiable owner.

## Responsibilities

The threat model owns the enumeration of boundaries and residual risks. The mechanisms are owned by
`crates/ma-secure` (`Secret<T>`, `Content`, `LogValue`, `SecurityDescriptor`), by the inventory check in
`crates/ma-secure/tests/egress_inventory.rs`, and by the contracts named under *Related Decisions*.

## Boundaries

| Boundary | Crosses | Enforced by |
| --- | --- | --- |
| operating system → engine | process, package identity, audio session and microphone facts | closed signal envelope; no free-text field (`contracts/signal/signal-envelope.schema.json`) |
| browser extension → engine | non-authoritative tab signals over the loopback listener | token, pinned origin, freshness; extension evidence alone never starts a recording |
| interface host → engine | JSON-RPC commands and events | owner-only DACL, first-pipe-instance, client SID comparison, per-channel server authenticity |
| engine → processor host child | staged inputs, argument vector, secrets by environment block or stdin | never `argv`; per-job child under a job object |
| product → the user's accounts | exported transcripts, summaries and audio | `egress-inventory.toml`, `egress_audit` rows, credentials read on demand |
| distribution → product | installers, update and adapter manifests | code-signed installer, Ed25519-signed manifests verified before use |

## Invariants

- No secret value is serialized by a general-purpose serializer or written to any file the application
  writes; `Secret<T>` has no `Display` and no `Serialize`, and this is a compile-fail test.
- The diagnostic bundle of any run contains no meeting content and no secret; `Content` is not a
  `LogValue`, error `Display` implementations elide their payload, panic paths are scrubbed to
  root-relative form.
- The engine pipe never exists with a permissive DACL; the descriptor builder produces exactly one ACE,
  granting the owner.
- Every reachable host is declared with a closed owner; there is no `first_party` owner.

## Collaboration

`ma-ipc` and `ma-ext-channel` build their descriptors through `ma-secure::acl`. Every processor and
destination reads credentials through the `CredentialStore` trait and receives them as `Secret<T>`.
Destination adapters declare their hosts in their manifests' `egress_hosts`, which the inventory check reads.

## Failure Responsibility

A missing credential is `NeedsAuthentication` and disables the feature with the reason surfaced. An
unreachable credential store disables every dependent feature naming the store. A signature check that
cannot complete is a mismatch for a `release` client. A notification accepted by the platform but not
rendered is indistinguishable from a delivered one at the API surface and is accepted as a residual risk,
because refusing to record whenever delivery cannot be proved would suppress recording on any machine with
an aggressive notification policy.

## Variability

Fixed: the three integration owners, the owner-only descriptor shape, and the two carrier types. Free: how
`Secret<T>` zeroizes its buffer, and which platform notification surface the consent rule uses.

## Conformance

`cargo test -p ma-secure` (compile-fail witnesses, descriptor shape, elided parse errors, typed missing
credential) and `cargo test -p ma-secure --test egress_inventory`. The planted-marker scan over every file
the application writes runs on the Windows tier (`v-credential-no-secret-in-any-written-file`,
`v-redaction-marker-scan`).

## Related Decisions

adr-20260903-threat-model-and-credential-policy, adr-20260903-desktop-stack-and-ipc,
adr-20260903-extension-localhost-channel-trust, adr-20260903-update-and-manifest-distribution.
