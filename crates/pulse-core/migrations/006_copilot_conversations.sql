CREATE TABLE copilot_conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_copilot_conversations_updated ON copilot_conversations(updated_at DESC);

CREATE TABLE copilot_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES copilot_conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
  content TEXT NOT NULL,
  backend TEXT,
  task_refs_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);

CREATE INDEX idx_copilot_messages_conversation_created
  ON copilot_messages(conversation_id, created_at ASC);
