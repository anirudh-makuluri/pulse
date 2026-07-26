-- Durable local outbox for explicitly opt-in cloud synchronization.
-- Entries remain local until a sync worker has received a successful response.

CREATE TABLE sync_outbox (
  id TEXT PRIMARY KEY,
  record_type TEXT NOT NULL,
  record_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT NOT NULL,
  last_error TEXT,
  delivered_at TEXT
);

CREATE INDEX idx_sync_outbox_ready
  ON sync_outbox(delivered_at, next_attempt_at, created_at);
