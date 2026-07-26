-- Pulse durable activity-memory schema for CockroachDB.
--
-- Run this once against a dedicated `pulse` database. The sync API owns writes;
-- agent/MCP identities should be granted SELECT only. `embedding` uses the
-- configured model's fixed dimension (384 for all-MiniLM-L6-v2) and cosine distance.

CREATE TABLE IF NOT EXISTS pulse_activities (
  id UUID PRIMARY KEY,
  title STRING NOT NULL,
  status STRING NOT NULL,
  source STRING NOT NULL,
  project STRING,
  notes STRING,
  suggested_next_action STRING,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_activities_updated_idx
  ON pulse_activities (updated_at DESC);

CREATE TABLE IF NOT EXISTS pulse_sessions (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  agent STRING,
  application STRING,
  repository_path STRING,
  external_id STRING,
  source_ref STRING,
  started_at TIMESTAMPTZ NOT NULL,
  ended_at TIMESTAMPTZ,
  metadata JSONB NOT NULL DEFAULT '{}',
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_sessions_activity_started_idx
  ON pulse_sessions (activity_id, started_at DESC);

CREATE TABLE IF NOT EXISTS pulse_events (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  session_id UUID REFERENCES pulse_sessions(id) ON DELETE SET NULL,
  kind STRING NOT NULL,
  summary STRING NOT NULL,
  payload JSONB,
  source_ref STRING,
  occurred_at TIMESTAMPTZ NOT NULL,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_events_activity_occurred_idx
  ON pulse_events (activity_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS pulse_checkpoints (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  session_id UUID REFERENCES pulse_sessions(id) ON DELETE SET NULL,
  summary STRING NOT NULL,
  decisions JSONB NOT NULL DEFAULT '[]',
  failures JSONB NOT NULL DEFAULT '[]',
  next_actions JSONB NOT NULL DEFAULT '[]',
  source_ref STRING,
  created_at TIMESTAMPTZ NOT NULL,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_checkpoints_activity_created_idx
  ON pulse_checkpoints (activity_id, created_at DESC);

CREATE TABLE IF NOT EXISTS pulse_reminders (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  title STRING NOT NULL,
  due_at TIMESTAMPTZ NOT NULL,
  status STRING NOT NULL,
  context JSONB NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_reminders_activity_due_idx
  ON pulse_reminders (activity_id, due_at DESC);

CREATE TABLE IF NOT EXISTS pulse_memories (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  checkpoint_id UUID REFERENCES pulse_checkpoints(id) ON DELETE SET NULL,
  kind STRING NOT NULL,
  content STRING NOT NULL,
  provenance JSONB NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_memories_activity_created_idx
  ON pulse_memories (activity_id, created_at DESC);

CREATE TABLE IF NOT EXISTS pulse_artifacts (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  session_id UUID REFERENCES pulse_sessions(id) ON DELETE SET NULL,
  kind STRING NOT NULL,
  name STRING NOT NULL,
  object_key STRING,
  content_type STRING,
  size_bytes INT8,
  checksum STRING,
  metadata JSONB NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL,
  synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS pulse_artifacts_activity_created_idx
  ON pulse_artifacts (activity_id, created_at DESC);

-- One row per searchable activity/checkpoint/memory/reminder text. Update the
-- dimension if the configured embedding model differs from MiniLM's 384
-- dimensions before applying this schema.
CREATE TABLE IF NOT EXISTS pulse_embeddings (
  id UUID PRIMARY KEY,
  activity_id UUID NOT NULL REFERENCES pulse_activities(id) ON DELETE CASCADE,
  source_type STRING NOT NULL,
  source_id UUID NOT NULL,
  content STRING NOT NULL,
  embedding VECTOR(384) NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (source_type, source_id)
);

CREATE INDEX IF NOT EXISTS pulse_embeddings_activity_idx
  ON pulse_embeddings (activity_id);

-- CockroachDB vector indexes must be explicitly enabled by an administrator:
-- SET CLUSTER SETTING feature.vector_index.enabled = true;
-- CREATE VECTOR INDEX IF NOT EXISTS pulse_embeddings_cosine_idx
--   ON pulse_embeddings (embedding vector_cosine_ops);
