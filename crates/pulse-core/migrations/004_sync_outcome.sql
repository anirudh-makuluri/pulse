-- AI-observed outcome is deliberately separate from the user's task state.
-- A completed session may still be in the Inbox awaiting human review.
ALTER TABLE tasks ADD COLUMN sync_outcome TEXT
  CHECK (sync_outcome IN ('in_progress', 'completed', 'unclear'));
ALTER TABLE tasks ADD COLUMN sync_outcome_confidence REAL
  CHECK (sync_outcome_confidence IS NULL OR (sync_outcome_confidence >= 0 AND sync_outcome_confidence <= 1));
