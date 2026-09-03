---
id: adr-20260903-local-store-and-artifact-layout
kind: adr
title: SQLite pinned under local application data, artifacts under a relocatable root,
  deletion by purge and tombstone
summary: The database is a projection pinned under LOCALAPPDATA; artifacts live under
  a user-configurable root addressed as root identifier plus relative path; deletion
  is two-phase with an idempotent purge and a content-free tombstone.
status: accepted
created: '2026-09-03'
decision_makers:
- take
consequences:
  positive:
  - Moving the artifact root is a settings change, not a migration.
  - A hostile or very long meeting title can never reach the filesystem.
  - Deletion has one owner, converges after interruption, and can be asserted complete
    by scanning the root and the tables.
  - Losing the database never loses a recording.
  negative:
  - Identifier-only paths make the artifact tree unreadable to a human browsing it
    without the library.
  - Two writer processes keep cross-process write contention that a single-writer
    design would remove.
  - The tombstone is a permanent row per deleted meeting, so the table grows without
    bound over the product's life.
  neutral:
  - The grace period exists as a configuration point with no policy attached until
    Phase 2 sets one.
  - The released-version set the forward-only migration test ranges over is currently
    empty, so the test grows a case per release rather than being written once against
    a corpus that exists.
  - Remote objects outliving a deleted meeting is a user-visible behaviour that the
    interface has to explain.
confirmation: cargo test -p ma-store no_absolute_artifact_path_is_stored (T0), root_relocation_preserves_references
  (T1), purge_leaves_only_tombstone (T1), purge_rerun_is_idempotent (T1), migrate_from_every_released_version
  (T1).
tags:
- data-model
- storage
- retention
owners:
- take
relations:
- {type: originatedFrom, target: change-20260903-phase0-repository-and-contracts}
source_paths:
- PLAN.md
updated: '2026-09-03'
---

## Context

The user decision fixes SQLite in write-ahead-logging mode under `%LOCALAPPDATA%\MeetingAssistant\db\` and artifacts under `%LOCALAPPDATA%\MeetingAssistant\meetings\<meeting-id>\` with a user-configurable root. Two consequences the decision does not state have to be fixed here, because implementations would otherwise differ on observable behaviour.

First, a configurable artifact root may be a network share or a removable drive, which the database must not be. Second, if the chunk directory is the truth and the database is a projection, then deleting a meeting crosses that split and reaches five independently survivable places — chunk files, the consolidated audio, derived artifacts, database rows, and the audit rows naming remote objects. PLAN section 3 assigns retention and deletion to the application, and PLAN section 8 defers only the retention *values* to Phase 2.

## Decision

The database stays pinned under the local application-data directory regardless of the configured artifact root, and is treated as a **projection**: the authoritative record of captured audio is the chunk directory, so losing the database degrades the library and never the recording.

Artifacts are addressed as `(root_id, relative_path)` where every path segment is a generated identifier. No absolute path and no user-supplied text ever reaches the filesystem, so relocating the root updates one row and invalidates no reference, and a hostile meeting title cannot become a path.

Writer ownership is declared per table family and enforced by a connection role, with exactly two writer processes: the engine writes `session`, `workflow`, `export` and `tombstone`; the interface writes `settings`.

The store's own compatibility discipline is fixed here rather than in a separate contract: SQLite `user_version` is the schema-version carrier, migrations are ordered and **forward-only** with no down-migration, they are tested from every released version and from empty, and a database whose `user_version` is newer than the binary understands is a typed refusal to open — never a best-effort read of the columns that happen to be recognised. It belongs next to writer ownership because the refusal, the migration order and the role check are all enforced on the same connection-open path.

Deletion is two-phase. `meeting.delete` sets `deleted_at` in one transaction, which hides the meeting from every view and from workflow enqueue and requests cancellation of its in-flight steps and exports. A **purge job** then removes the meeting directory recursively, deletes derived rows, and inserts a `tombstone` carrying the meeting identifier, two timestamps and the identifiers of the remote objects this application created. The purge is idempotent and resumable from `deleted_at` alone. Remote objects are **never** deleted: they are the user's own files in the user's own account, and the tombstone exists so the interface can list them. A configurable grace period between the phases exists with **no default value**, which is Phase 2's decision per PLAN section 8.

## Alternatives considered

**Database beside the artifacts under the configurable root.** Keeps everything in one place and makes backup trivial. Rejected because a network share or an unplugged drive would take the application's state with it, and SQLite over a network share is not safe.

**Absolute artifact paths in the database.** Simplest addressing. Rejected because relocating the root would invalidate every stored reference, and moving the library is a feature the configurable root implies.

**Deletion as a single recursive directory removal.** Rejected because it leaves derived rows, export staging and audit rows behind, and because a kill mid-walk leaves a meeting that is neither present nor gone.

**Deleting remote exports along with the meeting.** Superficially tidier. Rejected as an unauthorized destructive act on files the user owns in accounts the application merely has scoped access to.

## Consequences

Recorded in the frontmatter as the tripolar set; restated here for readers.

**Positive.**

- Moving the artifact root is a settings change, not a migration.
- A hostile or very long meeting title can never reach the filesystem.
- Deletion has one owner, converges after interruption, and can be asserted complete by scanning the root and the tables.
- Losing the database never loses a recording.

**Negative.**

- Identifier-only paths make the artifact tree unreadable to a human browsing it without the library.
- Two writer processes keep cross-process write contention that a single-writer design would remove.
- The tombstone is a permanent row per deleted meeting, so the table grows without bound over the product's life.

**Neutral.**

- The grace period exists as a configuration point with no policy attached until Phase 2 sets one.
- The released-version set the forward-only migration test ranges over is currently empty, so the test grows a case per release rather than being written once against a corpus that exists.
- Remote objects outliving a deleted meeting is a user-visible behaviour that the interface has to explain.

## Confirmation

cargo test -p ma-store no_absolute_artifact_path_is_stored (T0), root_relocation_preserves_references (T1), purge_leaves_only_tombstone (T1), purge_rerun_is_idempotent (T1), migrate_from_every_released_version (T1).


{% transition from="proposed" to="accepted" date="2026-09-03" %}
consultation-phase0-20260903-1: user accepted all fifteen Phase 0 ADRs (2026-09-03)
{% /transition %}
