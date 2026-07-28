-- Internal checkpoint for explicit session sync. This is intentionally not a
-- user-facing work checkpoint: it prevents repeated LLM analysis of unchanged
-- transcripts and permanently associates an imported session with one task.
CREATE TABLE session_sync_state (
  external_id TEXT PRIMARY KEY,
  source TEXT NOT NULL CHECK (source IN ('claude', 'codex')),
  source_session_id TEXT NOT NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  content_fingerprint TEXT NOT NULL,
  source_mtime_ms INTEGER NOT NULL,
  source_size_bytes INTEGER NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('created', 'updated', 'no_actionable_work')),
  last_checked_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_session_sync_state_source_session
  ON session_sync_state(source, source_session_id);
CREATE INDEX idx_session_sync_state_task ON session_sync_state(task_id);
