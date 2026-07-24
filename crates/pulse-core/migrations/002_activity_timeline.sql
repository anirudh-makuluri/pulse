-- Local activity timeline. `tasks` remains the activity root in this phase.
-- This migration is additive: existing activity_events continue to serve the
-- inference pipeline, while `events` records the richer activity timeline.

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  agent TEXT,
  application TEXT,
  repository_path TEXT,
  external_id TEXT,
  source_ref TEXT,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  created_at TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_sessions_task_started ON sessions(task_id, started_at DESC);
CREATE UNIQUE INDEX idx_sessions_external_id_unique
  ON sessions(external_id) WHERE external_id IS NOT NULL;

CREATE TABLE events (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  kind TEXT NOT NULL,
  summary TEXT NOT NULL,
  payload_json TEXT,
  source_ref TEXT,
  occurred_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_events_task_occurred ON events(task_id, occurred_at DESC);
CREATE INDEX idx_events_session_occurred ON events(session_id, occurred_at DESC);

CREATE TABLE checkpoints (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  summary TEXT NOT NULL,
  decisions_json TEXT NOT NULL DEFAULT '[]',
  failures_json TEXT NOT NULL DEFAULT '[]',
  next_actions_json TEXT NOT NULL DEFAULT '[]',
  source_ref TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_checkpoints_task_created ON checkpoints(task_id, created_at DESC);
CREATE INDEX idx_checkpoints_session_created ON checkpoints(session_id, created_at DESC);

CREATE TABLE reminders (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  due_at TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'snoozed', 'done', 'cancelled')) DEFAULT 'pending',
  context_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE INDEX idx_reminders_due_pending ON reminders(status, due_at)
  WHERE status IN ('pending', 'snoozed');
CREATE INDEX idx_reminders_task ON reminders(task_id);

CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  checkpoint_id TEXT REFERENCES checkpoints(id) ON DELETE SET NULL,
  kind TEXT NOT NULL,
  content TEXT NOT NULL,
  provenance_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_memories_task_created ON memories(task_id, created_at DESC);
CREATE INDEX idx_memories_checkpoint ON memories(checkpoint_id);

CREATE TABLE artifacts (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  local_path TEXT,
  content_type TEXT,
  size_bytes INTEGER,
  checksum TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE INDEX idx_artifacts_task_created ON artifacts(task_id, created_at DESC);
CREATE INDEX idx_artifacts_session ON artifacts(session_id);
