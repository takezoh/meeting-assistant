---
id: adr-20260903-threat-model-and-credential-policy
kind: adr
title: Trust boundaries, single-place secret custody, type-level redaction and a build-time
  egress inventory
summary: Secrets live only in the Windows Credential Manager behind a non-printable
  wrapper, diagnostics cannot carry meeting content by type, and every reachable host
  is declared in a repository egress inventory.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - A secret cannot be printed by a general-purpose serializer, and the compiler enforces
    it.
  - Meeting content cannot reach a log by accident, and the leak scan proves it over
    a whole synthetic run.
  - Adding an undeclared outbound host fails continuous integration rather than passing
    review.
  - The control channel's own tests can run against unsigned development builds without
    weakening the installed-build rule.
  negative:
  - The secret wrapper and the content type are friction on every path that touches
    them, and developers will occasionally have to work around the type system rather
    than through it.
  - The egress inventory is a file that must be edited whenever a legitimate host
    is added, and a stale entry fails the build.
  - Two build channels mean two authentication paths to keep correct, and the development
    path is a genuine, if narrow, additional surface.
  neutral:
  - The audit table and the inventory are different artifacts answering different
    questions — what happened, and what may happen.
  - Operating-system endpoints are declared for completeness even though product code
    never contacts them.
confirmation: cargo test -p ma-secure --test compile_fail content_type_cannot_be_logged
  (T0), --test leak_scan diagnostic_bundle_contains_no_markers (T2), --test egress_inventory
  every_source_host_is_declared (T0); cargo test -p ma-ipc authz_build_channel_carveout
  (T1).
tags:
- security
- privacy
- threat-model
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The product records meetings, holds provider tokens, and talks to accounts the user owns. PLAN section 7 requires that secrets never appear in application files or logs, that diagnostics contain no meeting content, that every external send be locally auditable, and that no first-party backend appear on any path.

Two of those are the kind of promise that is kept for six months and then broken by one convenient log line or one added constant, so both are made structural rather than procedural.

## Decision

**Trust boundaries** are named explicitly: the operating-system user, the extension channel, external providers, and the update supply chain. Assets are meeting audio, transcripts, summaries and provider tokens.

**Secret custody.** Every secret is a wrapper type whose debug, display and serialization renderings are masked and whose buffer is zeroized on drop; the inner value is reachable only through an explicit exposure call at the transmission site. Secrets live in the Windows Credential Manager and nowhere else — no application file, database, artifact, log or process argument vector — and there is no secondary cache.

**Redaction is a type-level property, not a logging convention.** Meeting-derived text is a distinct type that cannot be logged, enforced by a compile-fail test, and a leak scan runs a synthetic session with planted secret and content markers and searches every file the application wrote, including panic output and parse errors.

**Transport authorization** for the control channel is an owner-only access control list, first-instance pipe creation against squatting, and a client security-identifier comparison before method dispatch, with the compile-time build channel deciding the server-authenticity rule: a release client requires the installed path and a valid signature, a development client accepts only same-user servers inside its own build tree. The carve-out is narrower than the release rule and unavailable to a release build, so it cannot be used to downgrade one.

**Egress is bounded at build time.** `egress-inventory.toml` declares every host any component may contact, each mapped to a closed owner enum — user account, distribution, or operating system — with no first-party value available to write. A host reachable from source or from a processor or destination manifest that is absent from the inventory fails the build, and a stale entry fails with a distinct code. At runtime every send additionally appends a local audit record of identifiers and counts only, and every audited host must be an inventory host.

## Alternatives considered

**Encrypted secrets in an application file.** Portable and easy to back up. Rejected because the decryption key has to live somewhere, and the operating system already solves this.

**Redaction by filtering at the logging call site.** Common and cheap. Rejected because it depends on every future call site remembering, and the failure is silent and discovered by an external party.

**Documenting the egress hosts in the threat model.** Rejected because a document does not fail a build, and the exit criterion it supports is exactly the one a well-intentioned telemetry addition violates.

**A runtime-configurable trust channel for development builds.** Rejected because a release build could then be talked into development behaviour by configuration.

**Skipping the signature check when a binary is unsigned.** Rejected because it lets an attacker downgrade a release client by planting an unsigned binary.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- A secret cannot be printed by a general-purpose serializer, and the compiler enforces it.
- Meeting content cannot reach a log by accident, and the leak scan proves it over a whole synthetic run.
- Adding an undeclared outbound host fails continuous integration rather than passing review.
- The control channel's own tests can run against unsigned development builds without weakening the installed-build rule.

**Negative.**

- The secret wrapper and the content type are friction on every path that touches them, and developers will occasionally have to work around the type system rather than through it.
- The egress inventory is a file that must be edited whenever a legitimate host is added, and a stale entry fails the build.
- Two build channels mean two authentication paths to keep correct, and the development path is a genuine, if narrow, additional surface.

**Neutral.**

- The audit table and the inventory are different artifacts answering different questions — what happened, and what may happen.
- Operating-system endpoints are declared for completeness even though product code never contacts them.

## Confirmation

cargo test -p ma-secure --test compile_fail content_type_cannot_be_logged (T0), --test leak_scan diagnostic_bundle_contains_no_markers (T2), --test egress_inventory every_source_host_is_declared (T0); cargo test -p ma-ipc authz_build_channel_carveout (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
