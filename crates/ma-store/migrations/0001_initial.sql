-- Schema version 1. Writer ownership per table family is enforced by the connection role
-- (contract-store-ownership); no table stores an absolute artifact path
-- (contract-artifact-addressing).

-- session family (writer: engine)
CREATE TABLE meeting (
  meeting_id   TEXT PRIMARY KEY,
  created_at   INTEGER NOT NULL,
  title        TEXT,
  deleted_at   INTEGER
);
CREATE TABLE session (
  session_id      TEXT PRIMARY KEY,
  meeting_id      TEXT NOT NULL REFERENCES meeting(meeting_id),
  state           TEXT NOT NULL,
  continues_from  TEXT,
  created_at      INTEGER NOT NULL
);
CREATE TABLE session_transition (
  transition_id  INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id     TEXT NOT NULL REFERENCES session(session_id),
  from_state     TEXT NOT NULL,
  to_state       TEXT NOT NULL,
  event          TEXT NOT NULL,
  cause_kind     TEXT NOT NULL,
  cause_refs     TEXT NOT NULL,
  at_unbiased_ms INTEGER NOT NULL
);
CREATE TABLE track (
  track_id            TEXT PRIMARY KEY,
  session_id          TEXT NOT NULL REFERENCES session(session_id),
  origin_wall_utc_ms  INTEGER NOT NULL,
  origin_monotonic_ns INTEGER NOT NULL,
  sample_rate         INTEGER NOT NULL CHECK (sample_rate > 0),
  channels            INTEGER NOT NULL,
  capture_mode        TEXT NOT NULL,
  contamination_risk  TEXT NOT NULL
);
CREATE TABLE chunk (
  chunk_id      TEXT PRIMARY KEY,
  track_id      TEXT NOT NULL REFERENCES track(track_id),
  seq           INTEGER NOT NULL,
  start_sample  INTEGER NOT NULL,
  len_samples   INTEGER NOT NULL CHECK (len_samples > 0),
  root_id       TEXT NOT NULL REFERENCES roots(root_id),
  relative_path TEXT NOT NULL CHECK (relative_path NOT LIKE '/%' AND relative_path NOT LIKE '\%' AND relative_path NOT LIKE '_:%' AND relative_path NOT LIKE '%..%'),
  UNIQUE (track_id, seq)
);
CREATE TABLE gap (
  gap_id      INTEGER PRIMARY KEY AUTOINCREMENT,
  track_id    TEXT NOT NULL REFERENCES track(track_id),
  from_sample INTEGER NOT NULL,
  to_sample   INTEGER NOT NULL CHECK (to_sample > from_sample),
  reason      TEXT NOT NULL
);

-- workflow family (writer: engine)
CREATE TABLE workflow_step (
  step_id     TEXT PRIMARY KEY,
  meeting_id  TEXT NOT NULL REFERENCES meeting(meeting_id),
  step_key    TEXT NOT NULL UNIQUE,
  processor   TEXT NOT NULL,
  version     TEXT NOT NULL,
  config_hash TEXT NOT NULL,
  status      TEXT NOT NULL,
  result_ref  TEXT
);
CREATE TABLE work_item (
  work_item_id TEXT PRIMARY KEY,
  step_id      TEXT NOT NULL REFERENCES workflow_step(step_id),
  ordinal      INTEGER NOT NULL,
  status       TEXT NOT NULL
);
CREATE TABLE effect_ledger (
  effect_id   TEXT PRIMARY KEY,
  meeting_id  TEXT NOT NULL REFERENCES meeting(meeting_id),
  step_id     TEXT REFERENCES workflow_step(step_id),
  kind            TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  state           TEXT NOT NULL CHECK (state IN ('intended', 'committed', 'abandoned')),
  remote_ref      TEXT,
  at_ms           INTEGER NOT NULL
);
CREATE TABLE artifact (
  artifact_id   TEXT PRIMARY KEY,
  meeting_id    TEXT NOT NULL REFERENCES meeting(meeting_id),
  kind          TEXT NOT NULL,
  root_id       TEXT NOT NULL REFERENCES roots(root_id),
  relative_path TEXT NOT NULL CHECK (relative_path NOT LIKE '/%' AND relative_path NOT LIKE '\%' AND relative_path NOT LIKE '_:%' AND relative_path NOT LIKE '%..%'),
  generation_id TEXT,
  created_at    INTEGER NOT NULL
);
CREATE TABLE generation (
  generation_id   TEXT PRIMARY KEY,
  meeting_id      TEXT NOT NULL REFERENCES meeting(meeting_id),
  artifact_id     TEXT NOT NULL REFERENCES artifact(artifact_id),
  step_id         TEXT NOT NULL REFERENCES workflow_step(step_id),
  processor_id    TEXT NOT NULL,
  model_id        TEXT NOT NULL,
  adapter_version TEXT NOT NULL,
  created_at      INTEGER NOT NULL
);
CREATE TABLE edit_overlay (
  overlay_id    TEXT PRIMARY KEY,
  meeting_id    TEXT NOT NULL REFERENCES meeting(meeting_id),
  artifact_id   TEXT NOT NULL REFERENCES artifact(artifact_id),
  target_kind   TEXT NOT NULL CHECK (target_kind IN ('speaker_label', 'transcript_text', 'summary_text')),
  anchor        TEXT NOT NULL,
  value_json    TEXT NOT NULL,
  edited_at     INTEGER NOT NULL,
  orphaned      INTEGER NOT NULL DEFAULT 0 CHECK (orphaned IN (0, 1))
);

-- export family (writer: engine)
CREATE TABLE export (
  export_id    TEXT PRIMARY KEY,
  meeting_id   TEXT NOT NULL REFERENCES meeting(meeting_id),
  destination  TEXT NOT NULL,
  status       TEXT NOT NULL,
  remote_id    TEXT
);
CREATE TABLE export_attempt (
  attempt_id  INTEGER PRIMARY KEY AUTOINCREMENT,
  export_id   TEXT NOT NULL REFERENCES export(export_id),
  started_at  INTEGER NOT NULL,
  outcome     TEXT
);
CREATE TABLE egress_audit (
  audit_id       INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id     TEXT REFERENCES meeting(meeting_id),
  destination_id TEXT NOT NULL,
  host           TEXT NOT NULL,
  purpose        TEXT NOT NULL,
  artifact_id    TEXT,
  bytes          INTEGER NOT NULL DEFAULT 0,
  remote_ref     TEXT,
  outcome        TEXT NOT NULL,
  at_ms          INTEGER NOT NULL
);

-- tombstone family (writer: engine purge job)
CREATE TABLE tombstone (
  meeting_id            TEXT PRIMARY KEY,
  created_at            INTEGER NOT NULL,
  deleted_at            INTEGER NOT NULL,
  purged_at             INTEGER NOT NULL,
  remote_resource_refs  TEXT NOT NULL
);

-- settings family (writer: interface host)
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL
);
CREATE TABLE app_mode_override (
  adapter_id TEXT PRIMARY KEY,
  mode       TEXT NOT NULL CHECK (mode IN ('auto', 'ask', 'manual'))
);
CREATE TABLE roots (
  root_id       TEXT PRIMARY KEY,
  absolute_path TEXT NOT NULL,
  is_default    INTEGER NOT NULL DEFAULT 0
);
