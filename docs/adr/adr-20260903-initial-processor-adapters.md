---
id: adr-20260903-initial-processor-adapters
kind: adr
title: Initial processor adapters and the contract every processor must satisfy
summary: whisper.cpp local, OpenAI speech-to-text external, sherpa-onnx diarization
  and Claude summarization, behind a contract fixing capability declaration, staged
  inputs, argument-vector invocation, provenance and child-process execution.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - A later adapter cannot introduce a shell, an unverified model or an undeclared
    host without failing a test.
  - Cancellation is bounded by construction because the work runs in a killable child.
  - Provenance on every output makes it possible to tell which model produced which
    transcript years later.
  negative:
  - Every job pays process-spawn latency and needs a framing protocol for progress
    and results.
  - Staging copies inputs into a per-job directory, which costs disk and time for
    large audio files.
  - The argument-vector rule makes some genuinely useful command-line tools awkward
    to adapt, because they expect a shell.
  neutral:
  - Phase 0 ships a scripted fake processor rather than any real adapter, and that
    fake must be maintained until the real ones land.
  - The Japanese and English restriction is a capability declaration, not a code restriction.
confirmation: cargo test -p ma-processor config_value_never_reaches_a_shell (T1),
  staging_dir_contains_only_declared_inputs (T1), unsupported_language_is_typed_refusal
  (T1), model_digest_mismatch_is_permanent_failure (T1).
tags:
- processing
- adapters
- security
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes the initial adapters: whisper.cpp `large-v3-turbo` for local transcription with GPU optional, the OpenAI speech-to-text API as the external option, sherpa-onnx speaker-embedding clustering for diarization of the loopback track, and the Claude Messages API with an OpenAI-compatible alternative for summarization. Japanese and English only. A command-line adapter is deferred past the MVP.

Phase 0 implements none of them. What it must fix is the contract they satisfy, because the contract decides whether a later adapter can smuggle a shell command, an unverified model or an undeclared network host into the product.

## Decision

A processor declares its capability — kind, languages, GPU need, maximum input length, streaming, egress hosts — and a request outside that capability is refused as `Unsupported` before any work begins. An unsupported language is a typed refusal, never a best-effort transcription.

Inputs are passed as **staged paths** in a per-job directory containing exactly the declared inputs, created with an owner-only access control list and removed when the job ends. External programs are launched with an argument **vector** built from a template declared in the signed processor manifest, where user configuration supplies only enumerated, typed values substituted as whole arguments. **There is no shell, ever**, and no configuration field becomes a command line. Secrets never appear in the argument vector, which is readable by other processes on Windows; they pass through the child's environment or standard input.

Every processor that loads a native inference library or executes an external program runs inside `ma-processor-host.exe`, one child per job, bounded by a job object with a 4 GiB cap and cancelled by termination after a five-second graceful window. Outputs carry provenance naming the processor, its version, the model identifier and the model digest, and a digest mismatch is a permanent failure rather than a silent run on an unverified model.

The failure taxonomy is closed: `Unsupported`, `InvalidInput`, `Retryable`, `Permanent`, `Cancelled`, `BudgetExceeded`, `HostCrashed`.

## Alternatives considered

**faster-whisper through a Python sidecar.** Often faster and easier to update. Rejected because it adds a Python runtime to the installer and a second packaging story for a local-first desktop product.

**Cloud-only transcription.** Removes the local performance problem entirely. Rejected because local-first operation is the product's premise, and PLAN requires that external transmission be explicit and consented.

**Accepting a shell command string as processor configuration.** The obvious way to support arbitrary command-line tools. Rejected outright: it is a remote code execution surface configured by a settings field, and the deferred command-line adapter is made admissible now precisely by establishing the argument-vector rule before it exists.

**Running native processors in-process.** Rejected because an abort in a native library would terminate the process that records audio.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- A later adapter cannot introduce a shell, an unverified model or an undeclared host without failing a test.
- Cancellation is bounded by construction because the work runs in a killable child.
- Provenance on every output makes it possible to tell which model produced which transcript years later.

**Negative.**

- Every job pays process-spawn latency and needs a framing protocol for progress and results.
- Staging copies inputs into a per-job directory, which costs disk and time for large audio files.
- The argument-vector rule makes some genuinely useful command-line tools awkward to adapt, because they expect a shell.

**Neutral.**

- Phase 0 ships a scripted fake processor rather than any real adapter, and that fake must be maintained until the real ones land.
- The Japanese and English restriction is a capability declaration, not a code restriction.

## Confirmation

cargo test -p ma-processor config_value_never_reaches_a_shell (T1), staging_dir_contains_only_declared_inputs (T1), unsupported_language_is_typed_refusal (T1), model_digest_mismatch_is_permanent_failure (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
