-- Pulse MVP schema baseline.
-- MVP migrations after this file are ADDITIVE ONLY.

CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

CREATE TABLE tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('Inbox','Today','Next','Waiting','Done')),
  source TEXT NOT NULL CHECK (source IN ('manual','claude','codex','unknown')),
  confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
  project TEXT,
  notes TEXT,
  suggested_next_action TEXT,
  dedup_key TEXT,
  source_session_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_updated ON tasks(updated_at);
CREATE INDEX idx_tasks_project ON tasks(project);
CREATE UNIQUE INDEX idx_tasks_dedup_unique ON tasks(dedup_key) WHERE dedup_key IS NOT NULL;

CREATE TABLE evidence (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  source_ref TEXT NOT NULL,
  snippet TEXT,
  metadata_json TEXT,
  observed_at TEXT NOT NULL
);

CREATE INDEX idx_evidence_task ON evidence(task_id);
CREATE INDEX idx_evidence_source_ref ON evidence(source_ref);

CREATE TABLE activity_events (
  id TEXT PRIMARY KEY,
  source TEXT NOT NULL,
  kind TEXT NOT NULL,
  raw_ref TEXT NOT NULL,
  payload_json TEXT,
  observed_at TEXT NOT NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL
);

CREATE INDEX idx_activity_observed ON activity_events(observed_at);
CREATE INDEX idx_activity_task ON activity_events(task_id);

CREATE TABLE summaries (
  id TEXT PRIMARY KEY,
  day TEXT NOT NULL UNIQUE,
  timezone_offset_minutes INTEGER NOT NULL,
  text TEXT NOT NULL,
  highlights_json TEXT NOT NULL DEFAULT '[]',
  evidence_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);

CREATE TABLE checkins (
  id TEXT PRIMARY KEY,
  task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
  question TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('still_active','is_done','next_step')),
  status TEXT NOT NULL CHECK (status IN ('open','answered')) DEFAULT 'open',
  answer_json TEXT,
  created_at TEXT NOT NULL,
  answered_at TEXT
);

CREATE INDEX idx_checkins_open ON checkins(status);

CREATE TABLE source_watermarks (
  source_ref TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  byte_offset INTEGER NOT NULL DEFAULT 0,
  last_processed_at TEXT NOT NULL
);
