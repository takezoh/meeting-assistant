---
id: design-credential-policy
kind: design
title: "Credential policy"
summary: Where secrets live, how they are read, how they reach a child process, and what happens when they are missing.
status: active
created: '2026-09-03'
scope_type: policy
responsibilities:
  - id: RESP-001
    statement: "Define the single place a secret may live and the single way it may be read."
  - id: RESP-002
    statement: "Define how a secret reaches a processor child process."
  - id: RESP-003
    statement: "Define the typed outcome of a missing or unreadable credential."
invariants:
  - id: INV-001
    statement: "Secrets live only in the operating-system credential store under MeetingAssistant/<purpose>/<account>."
    enforcement: conformance
  - id: INV-002
    statement: "A secret is read on demand and never copied into configuration files, environment files, the database, the artifact tree, logs or process arguments (v-credential-argv-free, v-credential-no-secret-in-any-written-file)."
    enforcement: test
  - id: INV-003
    statement: "The inner value of a Secret<T> is reachable only through an explicit expose() at the transmission site (v-credential-type-not-displayable)."
    enforcement: test
boundaries:
  provides:
    - the CredentialStore trait and the Secret<T> carrier
  consumes:
    - the operating-system credential store
  forbidden:
    - a secondary secret cache anywhere
    - passing a secret through argv
    - an anonymous fallback when a credential is missing
variability:
  fixed:
    - the entry naming scheme
    - the on-demand read rule
    - the typed NeedsAuthentication outcome
  free:
    - the zeroization mechanism
    - whether a child receives the secret by environment block or by stdin
capabilities:
  - id: cap:credential-custody
    uniqueness: global
failure_responsibilities:
  - id: RESP-001
    statement: "Define the single place a secret may live and the single way it may be read."
  - id: RESP-002
    statement: "Define how a secret reaches a processor child process."
  - id: RESP-003
    statement: "Define the typed outcome of a missing or unreadable credential."
trust_boundaries:
  - id: TB-001
    statement: "credential store to product: the secret value crosses per use inside a Secret<T> wrapper, read on demand, so revocation takes effect at the next read."
compatibility_policies:
  - Renaming a purpose or account requires a migration of stored entries; entries are never silently re-keyed.
tags: [security, credentials]
owners: [take]
relations:
  - type: originatedFrom
    target: change-20260903-phase0-repository-and-contracts
  - type: dependsOn
    target: design-threat-model
source_paths:
  - crates/ma-secure/src/secret.rs
  - crates/ma-secure/src/credential_store.rs
---

## Purpose

A secret exists in exactly one place. This policy says where that place is, how the product reads from it,
how a secret reaches a child process, and what the product does when the secret is not there.

## Responsibilities

`crates/ma-secure` owns `Secret<T>` and the `CredentialStore` trait. Every processor and destination reads
its credential through the trait at the moment of use and transmits it through `expose()` at the call site
that sends it.

## Boundaries

Secrets live in Windows Credential Manager under `MeetingAssistant/<purpose>/<account>`. The Windows
implementation of the trait arrives with the first platform unit; Phase 0 ships the trait, the in-memory
store used by tests, and the compile-fail witnesses. A child processor that needs a key receives it in its
environment block or on stdin, never in `argv`, because `argv` is readable by other processes on Windows.

## Invariants

- `Secret<T>` renders `***` under `Debug`, has no `Display` and no raw `Serialize`, and zeroizes its buffer
  on drop.
- The set of files the application writes contains no secret bytes.
- Revocation takes effect at the next read because nothing caches the value.

## Collaboration

`contract-processor-interface` builds the child's environment from `Secret<T>` values; `contract-destination-export-idempotency` reads the destination credential per send; `contract-diagnostic-redaction` guarantees that a `Secret<T>` cannot reach a log field.

## Failure Responsibility

Missing credential: `NeedsAuthentication { purpose, account }`, feature disabled, reason visible in the
UI. Store unreachable: `StoreUnavailable { store }`, every dependent feature disabled naming the store.
There is no anonymous fallback and no silent "continue without export".

## Variability

Fixed: naming scheme, on-demand read, typed outcomes. Free: zeroization mechanism; environment block versus
stdin for child processes.

## Conformance

`cargo test -p ma-secure missing_credential_is_needs_authentication`,
`cargo test -p ma-secure --test compile_fail secret_cannot_be_displayed`, and on the Windows tier the
planted-marker scan over every written file.

## Related Decisions

adr-20260903-threat-model-and-credential-policy, adr-20260903-initial-processor-adapters.
