-- SQLite does not support altering CHECK constraints. Preserve all existing
-- rows while expanding the source values for Brave's opt-in local history.
CREATE TABLE tasks_new (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('Inbox','Today','Next','Waiting','Done')),
  source TEXT NOT NULL CHECK (source IN ('manual','brave','claude','codex','unknown')),
  confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
  project TEXT,
  notes TEXT,
  suggested_next_action TEXT,
  dedup_key TEXT,
  source_session_id TEXT,
  sync_outcome TEXT CHECK (sync_outcome IN ('in_progress', 'completed', 'unclear')),
  sync_outcome_confidence REAL CHECK (sync_outcome_confidence IS NULL OR (sync_outcome_confidence >= 0 AND sync_outcome_confidence <= 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT
);
INSERT INTO tasks_new (
  id, title, status, source, confidence, project, notes,
  suggested_next_action, dedup_key, source_session_id,
  sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
)
SELECT
  id, title, status, source, confidence, project, notes,
  suggested_next_action, dedup_key, source_session_id,
  sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_updated ON tasks(updated_at);
CREATE INDEX idx_tasks_project ON tasks(project);
CREATE UNIQUE INDEX idx_tasks_dedup_unique ON tasks(dedup_key) WHERE dedup_key IS NOT NULL;

CREATE TABLE session_sync_state_new (
  external_id TEXT PRIMARY KEY,
  source TEXT NOT NULL CHECK (source IN ('brave', 'claude', 'codex')),
  source_session_id TEXT NOT NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  content_fingerprint TEXT NOT NULL,
  source_mtime_ms INTEGER NOT NULL,
  source_size_bytes INTEGER NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('created', 'updated', 'no_actionable_work')),
  last_checked_at TEXT NOT NULL
);
INSERT INTO session_sync_state_new SELECT * FROM session_sync_state;
DROP TABLE session_sync_state;
ALTER TABLE session_sync_state_new RENAME TO session_sync_state;
CREATE UNIQUE INDEX idx_session_sync_state_source_session
  ON session_sync_state(source, source_session_id);
CREATE INDEX idx_session_sync_state_task ON session_sync_state(task_id);
