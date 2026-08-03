use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{PulseError, Result};
use crate::models::{
    ActivityEvent, Artifact, CheckIn, CheckInKind, CheckInStatus, Checkpoint, CopilotConversation,
    CopilotMessage, Evidence, Memory,
    NewActivityEvent, NewArtifact, NewCheckIn, NewCheckpoint, NewEvidence, NewMemory, NewReminder,
    NewSession, NewSessionSyncState, NewTask, Reminder, ReminderStatus, Session, SessionSyncState,
    SourceWatermark, Summary, SyncOutboxItem, SyncOutcome, Task, TaskSource, TaskStatus,
    TaskUpdate,
};
use crate::state::validate_transition;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn create_copilot_conversation(&self, title: &str) -> Result<CopilotConversation> {
        let title = title.trim();
        if title.is_empty() {
            return Err(PulseError::Validation("copilot conversation title must not be empty".into()));
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO copilot_conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.to_string(), title, now.to_rfc3339(), now.to_rfc3339()],
        )?;
        Ok(CopilotConversation { id, title: title.into(), created_at: now, updated_at: now })
    }

    pub fn get_copilot_conversation(&self, id: Uuid) -> Result<Option<CopilotConversation>> {
        self.conn.query_row(
            "SELECT id, title, created_at, updated_at FROM copilot_conversations WHERE id = ?1",
            params![id.to_string()],
            map_copilot_conversation,
        ).optional().map_err(Into::into)
    }

    pub fn list_recent_copilot_conversations(&self, limit: usize) -> Result<Vec<CopilotConversation>> {
        let mut statement = self.conn.prepare(
            "SELECT id, title, created_at, updated_at FROM copilot_conversations ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit.clamp(1, 50) as i64], map_copilot_conversation)?;
        let mut conversations = Vec::new();
        for row in rows {
            conversations.push(row?);
        }
        Ok(conversations)
    }

    pub fn append_copilot_message(
        &self,
        conversation_id: Uuid,
        role: &str,
        content: &str,
        backend: Option<&str>,
        task_refs_json: &str,
    ) -> Result<CopilotMessage> {
        if !matches!(role, "user" | "assistant") {
            return Err(PulseError::Validation("invalid copilot message role".into()));
        }
        if self.get_copilot_conversation(conversation_id)?.is_none() {
            return Err(PulseError::Validation("copilot conversation not found".into()));
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO copilot_messages (id, conversation_id, role, content, backend, task_refs_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id.to_string(), conversation_id.to_string(), role, content, backend, task_refs_json, now.to_rfc3339()],
        )?;
        self.conn.execute(
            "UPDATE copilot_conversations SET updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), conversation_id.to_string()],
        )?;
        Ok(CopilotMessage {
            id,
            conversation_id,
            role: role.into(),
            content: content.into(),
            backend: backend.map(str::to_owned),
            task_refs_json: task_refs_json.into(),
            created_at: now,
        })
    }

    pub fn list_copilot_messages(&self, conversation_id: Uuid) -> Result<Vec<CopilotMessage>> {
        let mut statement = self.conn.prepare(
            "SELECT id, conversation_id, role, content, backend, task_refs_json, created_at FROM copilot_messages WHERE conversation_id = ?1 ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map(params![conversation_id.to_string()], map_copilot_message)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    pub fn create_task(&self, new: NewTask) -> Result<Task> {
        let title = new.title.trim();
        if title.is_empty() {
            return Err(PulseError::Validation(
                "task title must not be empty".into(),
            ));
        }
        if let Some(c) = new.confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(PulseError::Validation("confidence must be in [0,1]".into()));
            }
        }
        if let Some(c) = new.sync_outcome_confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(PulseError::Validation(
                    "sync outcome confidence must be in [0,1]".into(),
                ));
            }
        }

        let now = Utc::now();
        let id = Uuid::new_v4();
        let completed_at = if new.status == TaskStatus::Done {
            Some(now)
        } else {
            None
        };

        self.conn.execute(
            r#"
            INSERT INTO tasks (
              id, title, status, source, confidence, project, notes,
              suggested_next_action, dedup_key, source_session_id,
              sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
            ) VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6, ?7,
              ?8, ?9, ?10,
              ?11, ?12, ?13, ?14, ?15
            )
            "#,
            params![
                id.to_string(),
                title,
                new.status.as_str(),
                new.source.as_str(),
                new.confidence,
                new.project,
                new.notes,
                new.suggested_next_action,
                new.dedup_key,
                new.source_session_id,
                new.sync_outcome.map(|outcome| outcome.as_str()),
                new.sync_outcome_confidence,
                now.to_rfc3339(),
                now.to_rfc3339(),
                completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

        let task = self
            .get_task(id)?
            .ok_or_else(|| PulseError::TaskNotFound(id.to_string()))?;
        self.enqueue_sync_upsert("activity", task.id, &task)?;
        Ok(task)
    }

    pub fn get_task(&self, id: Uuid) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, title, status, source, confidence, project, notes,
                   suggested_next_action, dedup_key, source_session_id,
                   sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
            FROM tasks WHERE id = ?1
            "#,
        )?;
        let task = stmt
            .query_row(params![id.to_string()], map_task)
            .optional()?;
        Ok(task)
    }

    pub fn list_tasks(&self, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        if let Some(status) = status {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, title, status, source, confidence, project, notes,
                       suggested_next_action, dedup_key, source_session_id,
                       sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
                FROM tasks WHERE status = ?1
                ORDER BY updated_at DESC
                "#,
            )?;
            let rows = stmt.query_map(params![status.as_str()], map_task)?;
            for row in rows {
                tasks.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, title, status, source, confidence, project, notes,
                       suggested_next_action, dedup_key, source_session_id,
                       sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
                FROM tasks
                ORDER BY updated_at DESC
                "#,
            )?;
            let rows = stmt.query_map([], map_task)?;
            for row in rows {
                tasks.push(row?);
            }
        }
        Ok(tasks)
    }

    pub fn update_task(&self, id: Uuid, update: TaskUpdate) -> Result<Task> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| PulseError::TaskNotFound(id.to_string()))?;

        if let Some(title) = update.title {
            let t = title.trim();
            if t.is_empty() {
                return Err(PulseError::Validation(
                    "task title must not be empty".into(),
                ));
            }
            task.title = t.to_string();
        }
        if let Some(status) = update.status {
            if status != task.status {
                validate_transition(task.status, status)?;
                task.status = status;
                if status == TaskStatus::Done {
                    task.completed_at = Some(Utc::now());
                } else if task.completed_at.is_some() && status != TaskStatus::Done {
                    // reopen
                    task.completed_at = None;
                }
            }
        }
        if let Some(notes) = update.notes {
            task.notes = Some(notes);
        }
        if let Some(project) = update.project {
            task.project = Some(project);
        }
        if let Some(next) = update.suggested_next_action {
            task.suggested_next_action = Some(next);
        }
        if let Some(c) = update.confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(PulseError::Validation("confidence must be in [0,1]".into()));
            }
            task.confidence = Some(c);
        }
        if let Some(outcome) = update.sync_outcome {
            task.sync_outcome = Some(outcome);
        }
        if let Some(confidence) = update.sync_outcome_confidence {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(PulseError::Validation(
                    "sync outcome confidence must be in [0,1]".into(),
                ));
            }
            task.sync_outcome_confidence = Some(confidence);
        }

        task.updated_at = Utc::now();

        self.conn.execute(
            r#"
            UPDATE tasks SET
              title = ?2,
              status = ?3,
              confidence = ?4,
              project = ?5,
              notes = ?6,
              suggested_next_action = ?7,
              sync_outcome = ?8,
              sync_outcome_confidence = ?9,
              updated_at = ?10,
              completed_at = ?11
            WHERE id = ?1
            "#,
            params![
                id.to_string(),
                task.title,
                task.status.as_str(),
                task.confidence,
                task.project,
                task.notes,
                task.suggested_next_action,
                task.sync_outcome.map(|outcome| outcome.as_str()),
                task.sync_outcome_confidence,
                task.updated_at.to_rfc3339(),
                task.completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

        self.enqueue_sync_upsert("activity", task.id, &task)?;
        Ok(task)
    }

    pub fn set_status(&self, id: Uuid, status: TaskStatus) -> Result<Task> {
        self.update_task(
            id,
            TaskUpdate {
                status: Some(status),
                ..Default::default()
            },
        )
    }

    pub fn mark_done(&self, id: Uuid) -> Result<Task> {
        self.set_status(id, TaskStatus::Done)
    }

    pub fn add_evidence(&self, new: NewEvidence) -> Result<Evidence> {
        // ensure task exists
        if self.get_task(new.task_id)?.is_none() {
            return Err(PulseError::TaskNotFound(new.task_id.to_string()));
        }
        let id = Uuid::new_v4();
        self.conn.execute(
            r#"
            INSERT INTO evidence (id, task_id, kind, source_ref, snippet, metadata_json, observed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id.to_string(),
                new.task_id.to_string(),
                new.kind,
                new.source_ref,
                new.snippet,
                new.metadata_json,
                new.observed_at.to_rfc3339(),
            ],
        )?;
        Ok(Evidence {
            id,
            task_id: new.task_id,
            kind: new.kind,
            source_ref: new.source_ref,
            snippet: new.snippet,
            metadata_json: new.metadata_json,
            observed_at: new.observed_at,
        })
    }

    pub fn list_evidence(&self, task_id: Uuid) -> Result<Vec<Evidence>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_id, kind, source_ref, snippet, metadata_json, observed_at
            FROM evidence WHERE task_id = ?1
            ORDER BY observed_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![task_id.to_string()], |row| {
            Ok(Evidence {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                task_id: parse_uuid(row.get::<_, String>(1)?)?,
                kind: row.get(2)?,
                source_ref: row.get(3)?,
                snippet: row.get(4)?,
                metadata_json: row.get(5)?,
                observed_at: parse_dt(&row.get::<_, String>(6)?)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn find_by_dedup_key(&self, dedup_key: &str) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, title, status, source, confidence, project, notes,
                   suggested_next_action, dedup_key, source_session_id,
                   sync_outcome, sync_outcome_confidence, created_at, updated_at, completed_at
            FROM tasks WHERE dedup_key = ?1
            "#,
        )?;
        let task = stmt.query_row(params![dedup_key], map_task).optional()?;
        Ok(task)
    }

    pub fn get_watermark(&self, source_ref: &str) -> Result<Option<SourceWatermark>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT source_ref, path, size_bytes, mtime_ms, byte_offset, last_processed_at
            FROM source_watermarks WHERE source_ref = ?1
            "#,
        )?;
        let row = stmt
            .query_row(params![source_ref], |row| {
                Ok(SourceWatermark {
                    source_ref: row.get(0)?,
                    path: row.get(1)?,
                    size_bytes: row.get(2)?,
                    mtime_ms: row.get(3)?,
                    byte_offset: row.get(4)?,
                    last_processed_at: parse_dt(&row.get::<_, String>(5)?)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn upsert_watermark(&self, wm: &SourceWatermark) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO source_watermarks
              (source_ref, path, size_bytes, mtime_ms, byte_offset, last_processed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(source_ref) DO UPDATE SET
              path = excluded.path,
              size_bytes = excluded.size_bytes,
              mtime_ms = excluded.mtime_ms,
              byte_offset = excluded.byte_offset,
              last_processed_at = excluded.last_processed_at
            "#,
            params![
                wm.source_ref,
                wm.path,
                wm.size_bytes,
                wm.mtime_ms,
                wm.byte_offset,
                wm.last_processed_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_activity(
        &self,
        source: &str,
        kind: &str,
        raw_ref: &str,
        payload_json: Option<&str>,
        task_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute(
            r#"
            INSERT INTO activity_events
              (id, source, kind, raw_ref, payload_json, observed_at, task_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id.to_string(),
                source,
                kind,
                raw_ref,
                payload_json,
                now.to_rfc3339(),
                task_id.map(|t| t.to_string()),
            ],
        )?;
        Ok(id)
    }

    // --- Summaries ---

    pub fn upsert_summary(
        &self,
        day: &str,
        timezone_offset_minutes: i32,
        text: &str,
        highlights_json: &str,
        evidence_json: &str,
    ) -> Result<Summary> {
        let now = Utc::now();
        if let Some(existing) = self.get_summary(day)? {
            self.conn.execute(
                r#"
                UPDATE summaries SET
                  timezone_offset_minutes = ?2,
                  text = ?3,
                  highlights_json = ?4,
                  evidence_json = ?5,
                  created_at = ?6
                WHERE id = ?1
                "#,
                params![
                    existing.id.to_string(),
                    timezone_offset_minutes,
                    text,
                    highlights_json,
                    evidence_json,
                    now.to_rfc3339(),
                ],
            )?;
            return self
                .get_summary(day)?
                .ok_or_else(|| PulseError::Validation("summary missing after update".into()));
        }
        let id = Uuid::new_v4();
        self.conn.execute(
            r#"
            INSERT INTO summaries
              (id, day, timezone_offset_minutes, text, highlights_json, evidence_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id.to_string(),
                day,
                timezone_offset_minutes,
                text,
                highlights_json,
                evidence_json,
                now.to_rfc3339(),
            ],
        )?;
        self.get_summary(day)?
            .ok_or_else(|| PulseError::Validation("summary missing after insert".into()))
    }

    pub fn get_summary(&self, day: &str) -> Result<Option<Summary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, day, timezone_offset_minutes, text, highlights_json, evidence_json, created_at
            FROM summaries WHERE day = ?1
            "#,
        )?;
        let row = stmt
            .query_row(params![day], |row| {
                Ok(Summary {
                    id: parse_uuid(row.get::<_, String>(0)?)?,
                    day: row.get(1)?,
                    timezone_offset_minutes: row.get(2)?,
                    text: row.get(3)?,
                    highlights_json: row.get(4)?,
                    evidence_json: row.get(5)?,
                    created_at: parse_dt(&row.get::<_, String>(6)?)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    // --- Check-ins ---

    pub fn create_checkin(&self, new: NewCheckIn) -> Result<CheckIn> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute(
            r#"
            INSERT INTO checkins
              (id, task_id, question, kind, status, answer_json, created_at, answered_at)
            VALUES (?1, ?2, ?3, ?4, 'open', NULL, ?5, NULL)
            "#,
            params![
                id.to_string(),
                new.task_id.map(|t| t.to_string()),
                new.question,
                new.kind.as_str(),
                now.to_rfc3339(),
            ],
        )?;
        self.get_checkin(id)?
            .ok_or_else(|| PulseError::Validation("checkin missing after insert".into()))
    }

    pub fn get_checkin(&self, id: Uuid) -> Result<Option<CheckIn>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_id, question, kind, status, answer_json, created_at, answered_at
            FROM checkins WHERE id = ?1
            "#,
        )?;
        let row = stmt
            .query_row(params![id.to_string()], map_checkin)
            .optional()?;
        Ok(row)
    }

    pub fn list_checkins(&self, open_only: bool) -> Result<Vec<CheckIn>> {
        let mut out = Vec::new();
        if open_only {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, task_id, question, kind, status, answer_json, created_at, answered_at
                FROM checkins WHERE status = 'open' ORDER BY created_at DESC
                "#,
            )?;
            for row in stmt.query_map([], map_checkin)? {
                out.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, task_id, question, kind, status, answer_json, created_at, answered_at
                FROM checkins ORDER BY created_at DESC
                "#,
            )?;
            for row in stmt.query_map([], map_checkin)? {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn answer_checkin(&self, id: Uuid, answer_json: &str) -> Result<CheckIn> {
        let now = Utc::now();
        let n = self.conn.execute(
            r#"
            UPDATE checkins SET status = 'answered', answer_json = ?2, answered_at = ?3
            WHERE id = ?1 AND status = 'open'
            "#,
            params![id.to_string(), answer_json, now.to_rfc3339()],
        )?;
        if n == 0 {
            return Err(PulseError::Validation(format!(
                "check-in not found or already answered: {id}"
            )));
        }
        self.get_checkin(id)?
            .ok_or_else(|| PulseError::Validation("checkin missing".into()))
    }

    // --- Activity timeline ---

    pub fn create_session(&self, new: NewSession) -> Result<Session> {
        self.ensure_task_exists(new.task_id)?;
        validate_json(&new.metadata_json, "session metadata")?;
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        self.conn.execute(
            r#"INSERT INTO sessions
                (id, task_id, agent, application, repository_path, external_id, source_ref,
                 started_at, ended_at, created_at, metadata_json)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                id.to_string(),
                new.task_id.to_string(),
                new.agent,
                new.application,
                new.repository_path,
                new.external_id,
                new.source_ref,
                new.started_at.to_rfc3339(),
                new.ended_at.map(|t| t.to_rfc3339()),
                created_at.to_rfc3339(),
                new.metadata_json
            ],
        )?;
        let session = self
            .get_session(id)?
            .ok_or_else(|| PulseError::Validation("session missing after insert".into()))?;
        self.enqueue_sync_upsert("session", session.id, &session)?;
        Ok(session)
    }

    pub fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        self.conn.prepare(
            "SELECT id, task_id, agent, application, repository_path, external_id, source_ref, started_at, ended_at, created_at, metadata_json FROM sessions WHERE id = ?1",
        )?.query_row(params![id.to_string()], map_session).optional().map_err(Into::into)
    }

    /// Return a previously imported external session, if any. This makes an
    /// explicit sync idempotent even when the LLM chooses a slightly different
    /// title on a later run.
    pub fn get_session_by_external_id(&self, external_id: &str) -> Result<Option<Session>> {
        self.conn
            .prepare(
                "SELECT id, task_id, agent, application, repository_path, external_id, source_ref, started_at, ended_at, created_at, metadata_json FROM sessions WHERE external_id = ?1",
            )?
            .query_row(params![external_id], map_session)
            .optional()
            .map_err(Into::into)
    }

    /// Read the internal checkpoint used to avoid re-analyzing an unchanged
    /// external session with an LLM.
    pub fn get_session_sync_state(&self, external_id: &str) -> Result<Option<SessionSyncState>> {
        self.conn
            .prepare(
                "SELECT external_id, source, source_session_id, task_id, content_fingerprint, source_mtime_ms, source_size_bytes, result, last_checked_at FROM session_sync_state WHERE external_id = ?1",
            )?
            .query_row(params![external_id], map_session_sync_state)
            .optional()
            .map_err(Into::into)
    }

    /// Upsert a sync checkpoint. Its optional task ID is the durable
    /// one-session-to-one-task association used by explicit session sync.
    pub fn upsert_session_sync_state(
        &self,
        state: NewSessionSyncState,
    ) -> Result<SessionSyncState> {
        if !matches!(state.source.as_str(), "claude" | "codex") {
            return Err(PulseError::Validation(
                "session sync source must be claude or codex".into(),
            ));
        }
        if !matches!(
            state.result.as_str(),
            "created" | "updated" | "no_actionable_work"
        ) {
            return Err(PulseError::Validation("invalid session sync result".into()));
        }
        if state.external_id.trim().is_empty()
            || state.source_session_id.trim().is_empty()
            || state.content_fingerprint.trim().is_empty()
        {
            return Err(PulseError::Validation(
                "session sync checkpoint fields must not be empty".into(),
            ));
        }
        if let Some(task_id) = state.task_id {
            self.ensure_task_exists(task_id)?;
        }
        let external_id = state.external_id.clone();

        self.conn.execute(
            r#"
            INSERT INTO session_sync_state (
              external_id, source, source_session_id, task_id, content_fingerprint,
              source_mtime_ms, source_size_bytes, result, last_checked_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(external_id) DO UPDATE SET
              task_id = excluded.task_id,
              content_fingerprint = excluded.content_fingerprint,
              source_mtime_ms = excluded.source_mtime_ms,
              source_size_bytes = excluded.source_size_bytes,
              result = excluded.result,
              last_checked_at = excluded.last_checked_at
            "#,
            params![
                state.external_id,
                state.source,
                state.source_session_id,
                state.task_id.map(|id| id.to_string()),
                state.content_fingerprint,
                state.source_mtime_ms,
                state.source_size_bytes,
                state.result,
                state.last_checked_at.to_rfc3339(),
            ],
        )?;

        self.get_session_sync_state(&external_id)?.ok_or_else(|| {
            PulseError::Validation("session sync checkpoint missing after upsert".into())
        })
    }

    pub fn list_sessions(&self, task_id: Uuid) -> Result<Vec<Session>> {
        self.list_timeline_rows(
            "SELECT id, task_id, agent, application, repository_path, external_id, source_ref, started_at, ended_at, created_at, metadata_json FROM sessions WHERE task_id = ?1 ORDER BY started_at DESC",
            task_id,
            map_session,
        )
    }

    pub fn record_event(&self, new: NewActivityEvent) -> Result<ActivityEvent> {
        self.ensure_task_exists(new.task_id)?;
        self.ensure_session_belongs_to_task(new.session_id, new.task_id)?;
        validate_nonempty(&new.kind, "event kind")?;
        validate_nonempty(&new.summary, "event summary")?;
        if let Some(payload) = &new.payload_json {
            validate_json(payload, "event payload")?;
        }
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        self.conn.execute(
            "INSERT INTO events (id, task_id, session_id, kind, summary, payload_json, source_ref, occurred_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id.to_string(), new.task_id.to_string(), new.session_id.map(|v| v.to_string()),
                new.kind, new.summary, new.payload_json, new.source_ref, new.occurred_at.to_rfc3339(), created_at.to_rfc3339()],
        )?;
        let event = self
            .get_event(id)?
            .ok_or_else(|| PulseError::Validation("event missing after insert".into()))?;
        self.enqueue_sync_upsert("event", event.id, &event)?;
        Ok(event)
    }

    pub fn get_event(&self, id: Uuid) -> Result<Option<ActivityEvent>> {
        self.conn.prepare("SELECT id, task_id, session_id, kind, summary, payload_json, source_ref, occurred_at, created_at FROM events WHERE id = ?1")?
            .query_row(params![id.to_string()], map_event).optional().map_err(Into::into)
    }

    pub fn list_events(&self, task_id: Uuid) -> Result<Vec<ActivityEvent>> {
        self.list_timeline_rows(
            "SELECT id, task_id, session_id, kind, summary, payload_json, source_ref, occurred_at, created_at FROM events WHERE task_id = ?1 ORDER BY occurred_at DESC",
            task_id, map_event,
        )
    }

    pub fn create_checkpoint(&self, new: NewCheckpoint) -> Result<Checkpoint> {
        self.ensure_task_exists(new.task_id)?;
        self.ensure_session_belongs_to_task(new.session_id, new.task_id)?;
        validate_nonempty(&new.summary, "checkpoint summary")?;
        let decisions = serde_json::to_string(&new.decisions)
            .map_err(|e| PulseError::Validation(format!("checkpoint decisions: {e}")))?;
        let failures = serde_json::to_string(&new.failures)
            .map_err(|e| PulseError::Validation(format!("checkpoint failures: {e}")))?;
        let next_actions = serde_json::to_string(&new.next_actions)
            .map_err(|e| PulseError::Validation(format!("checkpoint next actions: {e}")))?;
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        self.conn.execute(
            "INSERT INTO checkpoints (id, task_id, session_id, summary, decisions_json, failures_json, next_actions_json, source_ref, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id.to_string(), new.task_id.to_string(), new.session_id.map(|v| v.to_string()),
                new.summary, decisions, failures, next_actions, new.source_ref, created_at.to_rfc3339()],
        )?;
        let checkpoint = self
            .get_checkpoint(id)?
            .ok_or_else(|| PulseError::Validation("checkpoint missing after insert".into()))?;
        self.enqueue_sync_upsert("checkpoint", checkpoint.id, &checkpoint)?;
        Ok(checkpoint)
    }

    pub fn get_checkpoint(&self, id: Uuid) -> Result<Option<Checkpoint>> {
        self.conn.prepare("SELECT id, task_id, session_id, summary, decisions_json, failures_json, next_actions_json, source_ref, created_at FROM checkpoints WHERE id = ?1")?
            .query_row(params![id.to_string()], map_checkpoint).optional().map_err(Into::into)
    }

    pub fn list_checkpoints(&self, task_id: Uuid) -> Result<Vec<Checkpoint>> {
        self.list_timeline_rows("SELECT id, task_id, session_id, summary, decisions_json, failures_json, next_actions_json, source_ref, created_at FROM checkpoints WHERE task_id = ?1 ORDER BY created_at DESC", task_id, map_checkpoint)
    }

    pub fn create_reminder(&self, new: NewReminder) -> Result<Reminder> {
        self.ensure_task_exists(new.task_id)?;
        validate_nonempty(&new.title, "reminder title")?;
        validate_json(&new.context_json, "reminder context")?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO reminders (id, task_id, title, due_at, status, context_json, created_at, updated_at, completed_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, NULL)",
            params![id.to_string(), new.task_id.to_string(), new.title, new.due_at.to_rfc3339(), new.context_json, now.to_rfc3339(), now.to_rfc3339()],
        )?;
        let reminder = self
            .get_reminder(id)?
            .ok_or_else(|| PulseError::Validation("reminder missing after insert".into()))?;
        self.enqueue_sync_upsert("reminder", reminder.id, &reminder)?;
        Ok(reminder)
    }

    pub fn get_reminder(&self, id: Uuid) -> Result<Option<Reminder>> {
        self.conn.prepare("SELECT id, task_id, title, due_at, status, context_json, created_at, updated_at, completed_at FROM reminders WHERE id = ?1")?
            .query_row(params![id.to_string()], map_reminder).optional().map_err(Into::into)
    }

    pub fn list_reminders(&self, task_id: Uuid) -> Result<Vec<Reminder>> {
        self.list_timeline_rows("SELECT id, task_id, title, due_at, status, context_json, created_at, updated_at, completed_at FROM reminders WHERE task_id = ?1 ORDER BY due_at ASC", task_id, map_reminder)
    }

    pub fn list_due_reminders(&self, due_before: chrono::DateTime<Utc>) -> Result<Vec<Reminder>> {
        let mut stmt = self.conn.prepare("SELECT id, task_id, title, due_at, status, context_json, created_at, updated_at, completed_at FROM reminders WHERE status IN ('pending', 'snoozed') AND due_at <= ?1 ORDER BY due_at ASC")?;
        let rows = stmt.query_map(params![due_before.to_rfc3339()], map_reminder)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn set_reminder_status(&self, id: Uuid, status: ReminderStatus) -> Result<Reminder> {
        let now = Utc::now();
        let completed_at = matches!(status, ReminderStatus::Done | ReminderStatus::Cancelled)
            .then_some(now.to_rfc3339());
        let changed = self.conn.execute(
            "UPDATE reminders SET status = ?2, updated_at = ?3, completed_at = ?4 WHERE id = ?1",
            params![
                id.to_string(),
                status.as_str(),
                now.to_rfc3339(),
                completed_at
            ],
        )?;
        if changed == 0 {
            return Err(PulseError::Validation(format!("reminder not found: {id}")));
        }
        let reminder = self
            .get_reminder(id)?
            .ok_or_else(|| PulseError::Validation("reminder missing after update".into()))?;
        self.enqueue_sync_upsert("reminder", reminder.id, &reminder)?;
        Ok(reminder)
    }

    pub fn snooze_reminder(&self, id: Uuid, due_at: chrono::DateTime<Utc>) -> Result<Reminder> {
        let reminder = self
            .get_reminder(id)?
            .ok_or_else(|| PulseError::Validation(format!("reminder not found: {id}")))?;
        if matches!(
            reminder.status,
            ReminderStatus::Done | ReminderStatus::Cancelled
        ) {
            return Err(PulseError::Validation(
                "cannot snooze a completed or cancelled reminder".into(),
            ));
        }
        let now = Utc::now();
        self.conn.execute(
            "UPDATE reminders SET due_at = ?2, status = 'snoozed', updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), due_at.to_rfc3339(), now.to_rfc3339()],
        )?;
        let reminder = self
            .get_reminder(id)?
            .ok_or_else(|| PulseError::Validation("reminder missing after snooze".into()))?;
        self.enqueue_sync_upsert("reminder", reminder.id, &reminder)?;
        Ok(reminder)
    }

    pub fn delete_task(&self, id: Uuid) -> Result<()> {
        if self
            .conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])?
            == 0
        {
            return Err(PulseError::TaskNotFound(id.to_string()));
        }
        self.enqueue_sync_delete("activity", id)?;
        Ok(())
    }

    pub fn create_memory(&self, new: NewMemory) -> Result<Memory> {
        self.ensure_task_exists(new.task_id)?;
        validate_nonempty(&new.kind, "memory kind")?;
        validate_nonempty(&new.content, "memory content")?;
        validate_json(&new.provenance_json, "memory provenance")?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        self.conn.execute("INSERT INTO memories (id, task_id, checkpoint_id, kind, content, provenance_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![id.to_string(), new.task_id.to_string(), new.checkpoint_id.map(|v| v.to_string()), new.kind, new.content, new.provenance_json, now.to_rfc3339(), now.to_rfc3339()])?;
        let memory = self
            .get_memory(id)?
            .ok_or_else(|| PulseError::Validation("memory missing after insert".into()))?;
        self.enqueue_sync_upsert("memory", memory.id, &memory)?;
        Ok(memory)
    }

    pub fn get_memory(&self, id: Uuid) -> Result<Option<Memory>> {
        self.conn.prepare("SELECT id, task_id, checkpoint_id, kind, content, provenance_json, created_at, updated_at FROM memories WHERE id = ?1")?.query_row(params![id.to_string()], map_memory).optional().map_err(Into::into)
    }

    pub fn list_memories(&self, task_id: Uuid) -> Result<Vec<Memory>> {
        self.list_timeline_rows("SELECT id, task_id, checkpoint_id, kind, content, provenance_json, created_at, updated_at FROM memories WHERE task_id = ?1 ORDER BY created_at DESC", task_id, map_memory)
    }

    pub fn create_artifact(&self, new: NewArtifact) -> Result<Artifact> {
        self.ensure_task_exists(new.task_id)?;
        self.ensure_session_belongs_to_task(new.session_id, new.task_id)?;
        validate_nonempty(&new.kind, "artifact kind")?;
        validate_nonempty(&new.name, "artifact name")?;
        validate_json(&new.metadata_json, "artifact metadata")?;
        if new.size_bytes.is_some_and(|size| size < 0) {
            return Err(PulseError::Validation(
                "artifact size must not be negative".into(),
            ));
        }
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        self.conn.execute("INSERT INTO artifacts (id, task_id, session_id, kind, name, local_path, content_type, size_bytes, checksum, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)", params![id.to_string(), new.task_id.to_string(), new.session_id.map(|v| v.to_string()), new.kind, new.name, new.local_path, new.content_type, new.size_bytes, new.checksum, new.metadata_json, created_at.to_rfc3339()])?;
        let artifact = self
            .get_artifact(id)?
            .ok_or_else(|| PulseError::Validation("artifact missing after insert".into()))?;
        self.enqueue_sync_upsert("artifact", artifact.id, &artifact)?;
        Ok(artifact)
    }

    pub fn get_artifact(&self, id: Uuid) -> Result<Option<Artifact>> {
        self.conn.prepare("SELECT id, task_id, session_id, kind, name, local_path, content_type, size_bytes, checksum, metadata_json, created_at FROM artifacts WHERE id = ?1")?.query_row(params![id.to_string()], map_artifact).optional().map_err(Into::into)
    }

    pub fn list_artifacts(&self, task_id: Uuid) -> Result<Vec<Artifact>> {
        self.list_timeline_rows("SELECT id, task_id, session_id, kind, name, local_path, content_type, size_bytes, checksum, metadata_json, created_at FROM artifacts WHERE task_id = ?1 ORDER BY created_at DESC", task_id, map_artifact)
    }

    /// Return a bounded batch of undelivered sync records that are eligible to
    /// retry. Keeping this local means task creation and reminders never wait
    /// on a network request.
    pub fn list_pending_sync(
        &self,
        limit: u32,
        now: chrono::DateTime<Utc>,
    ) -> Result<Vec<SyncOutboxItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, record_type, record_id, operation, payload_json, created_at, attempt_count, next_attempt_at, last_error FROM sync_outbox WHERE delivered_at IS NULL AND next_attempt_at <= ?1 ORDER BY created_at ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now.to_rfc3339(), i64::from(limit)], map_sync_outbox)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn pending_sync_count(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM sync_outbox WHERE delivered_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn mark_sync_delivered(
        &self,
        ids: &[Uuid],
        delivered_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE sync_outbox SET delivered_at = ?2, last_error = NULL WHERE id = ?1",
                params![id.to_string(), delivered_at.to_rfc3339()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn mark_sync_failed(
        &self,
        id: Uuid,
        error: &str,
        retry_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sync_outbox SET attempt_count = attempt_count + 1, last_error = ?2, next_attempt_at = ?3 WHERE id = ?1",
            params![id.to_string(), truncate_sync_error(error), retry_at.to_rfc3339()],
        )?;
        Ok(())
    }

    fn enqueue_sync_upsert<T: serde::Serialize>(
        &self,
        record_type: &str,
        record_id: Uuid,
        record: &T,
    ) -> Result<()> {
        let payload_json = serde_json::to_string(record)
            .map_err(|e| PulseError::Validation(format!("serialize sync payload: {e}")))?;
        self.enqueue_sync(record_type, record_id, "upsert", &payload_json)
    }

    fn enqueue_sync_delete(&self, record_type: &str, record_id: Uuid) -> Result<()> {
        self.enqueue_sync(
            record_type,
            record_id,
            "delete",
            &serde_json::json!({ "id": record_id }).to_string(),
        )
    }

    fn enqueue_sync(
        &self,
        record_type: &str,
        record_id: Uuid,
        operation: &str,
        payload_json: &str,
    ) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO sync_outbox (id, record_type, record_id, operation, payload_json, created_at, next_attempt_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![Uuid::new_v4().to_string(), record_type, record_id.to_string(), operation, payload_json, now.to_rfc3339(), now.to_rfc3339()],
        )?;
        Ok(())
    }

    fn ensure_task_exists(&self, task_id: Uuid) -> Result<()> {
        if self.get_task(task_id)?.is_none() {
            return Err(PulseError::TaskNotFound(task_id.to_string()));
        }
        Ok(())
    }

    fn ensure_session_belongs_to_task(
        &self,
        session_id: Option<Uuid>,
        task_id: Uuid,
    ) -> Result<()> {
        let Some(session_id) = session_id else {
            return Ok(());
        };
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| PulseError::Validation(format!("session not found: {session_id}")))?;
        if session.task_id != task_id {
            return Err(PulseError::Validation(
                "session must belong to the same task".into(),
            ));
        }
        Ok(())
    }

    fn list_timeline_rows<T, F>(&self, sql: &str, task_id: Uuid, map: F) -> Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![task_id.to_string()], map)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn resolve_checkin(&self, id_or_prefix: &str) -> Result<CheckIn> {
        let raw = id_or_prefix.trim();
        if let Ok(uuid) = Uuid::parse_str(raw) {
            return self
                .get_checkin(uuid)?
                .ok_or_else(|| PulseError::Validation(format!("check-in not found: {raw}")));
        }
        let needle = raw.to_ascii_lowercase();
        let all = self.list_checkins(false)?;
        let matches: Vec<_> = all
            .into_iter()
            .filter(|c| {
                c.id.as_hyphenated()
                    .to_string()
                    .to_ascii_lowercase()
                    .starts_with(&needle)
            })
            .collect();
        match matches.len() {
            0 => Err(PulseError::Validation(format!("check-in not found: {raw}"))),
            1 => Ok(matches.into_iter().next().unwrap()),
            _ => Err(PulseError::AmbiguousTaskId(raw.to_string())),
        }
    }

    /// Resolve a full UUID or unique id prefix to a task.
    pub fn resolve_task(&self, id_or_prefix: &str) -> Result<Task> {
        let raw = id_or_prefix.trim();
        if raw.is_empty() {
            return Err(PulseError::Validation("task id is empty".into()));
        }
        if let Ok(uuid) = Uuid::parse_str(raw) {
            return self
                .get_task(uuid)?
                .ok_or_else(|| PulseError::TaskNotFound(raw.to_string()));
        }
        let needle = raw.to_ascii_lowercase();
        let all = self.list_tasks(None)?;
        let matches: Vec<_> = all
            .into_iter()
            .filter(|t| {
                let hyphen = t.id.as_hyphenated().to_string().to_ascii_lowercase();
                let simple = t.id.simple().to_string().to_ascii_lowercase();
                hyphen.starts_with(&needle) || simple.starts_with(&needle)
            })
            .collect();
        match matches.len() {
            0 => Err(PulseError::TaskNotFound(raw.to_string())),
            1 => Ok(matches.into_iter().next().unwrap()),
            _ => Err(PulseError::AmbiguousTaskId(raw.to_string())),
        }
    }
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        agent: row.get(2)?,
        application: row.get(3)?,
        repository_path: row.get(4)?,
        external_id: row.get(5)?,
        source_ref: row.get(6)?,
        started_at: parse_dt(&row.get::<_, String>(7)?)?,
        ended_at: row
            .get::<_, Option<String>>(8)?
            .map(|v| parse_dt(&v))
            .transpose()?,
        created_at: parse_dt(&row.get::<_, String>(9)?)?,
        metadata_json: row.get(10)?,
    })
}

fn map_copilot_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<CopilotConversation> {
    Ok(CopilotConversation {
        id: parse_uuid(row.get(0)?)?,
        title: row.get(1)?,
        created_at: parse_dt(&row.get::<_, String>(2)?)?,
        updated_at: parse_dt(&row.get::<_, String>(3)?)?,
    })
}

fn map_copilot_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<CopilotMessage> {
    Ok(CopilotMessage {
        id: parse_uuid(row.get(0)?)?,
        conversation_id: parse_uuid(row.get(1)?)?,
        role: row.get(2)?,
        content: row.get(3)?,
        backend: row.get(4)?,
        task_refs_json: row.get(5)?,
        created_at: parse_dt(&row.get::<_, String>(6)?)?,
    })
}

fn map_session_sync_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSyncState> {
    Ok(SessionSyncState {
        external_id: row.get(0)?,
        source: row.get(1)?,
        source_session_id: row.get(2)?,
        task_id: row
            .get::<_, Option<String>>(3)?
            .map(parse_uuid)
            .transpose()?,
        content_fingerprint: row.get(4)?,
        source_mtime_ms: row.get(5)?,
        source_size_bytes: row.get(6)?,
        result: row.get(7)?,
        last_checked_at: parse_dt(&row.get::<_, String>(8)?)?,
    })
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActivityEvent> {
    Ok(ActivityEvent {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        session_id: row
            .get::<_, Option<String>>(2)?
            .map(parse_uuid)
            .transpose()?,
        kind: row.get(3)?,
        summary: row.get(4)?,
        payload_json: row.get(5)?,
        source_ref: row.get(6)?,
        occurred_at: parse_dt(&row.get::<_, String>(7)?)?,
        created_at: parse_dt(&row.get::<_, String>(8)?)?,
    })
}

fn map_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Checkpoint> {
    Ok(Checkpoint {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        session_id: row
            .get::<_, Option<String>>(2)?
            .map(parse_uuid)
            .transpose()?,
        summary: row.get(3)?,
        decisions: parse_json_vec(&row.get::<_, String>(4)?)?,
        failures: parse_json_vec(&row.get::<_, String>(5)?)?,
        next_actions: parse_json_vec(&row.get::<_, String>(6)?)?,
        source_ref: row.get(7)?,
        created_at: parse_dt(&row.get::<_, String>(8)?)?,
    })
}

fn map_reminder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reminder> {
    let status: String = row.get(4)?;
    Ok(Reminder {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        title: row.get(2)?,
        due_at: parse_dt(&row.get::<_, String>(3)?)?,
        status: ReminderStatus::parse(&status)
            .ok_or_else(|| bad_value(4, format!("bad reminder status {status}")))?,
        context_json: row.get(5)?,
        created_at: parse_dt(&row.get::<_, String>(6)?)?,
        updated_at: parse_dt(&row.get::<_, String>(7)?)?,
        completed_at: row
            .get::<_, Option<String>>(8)?
            .map(|v| parse_dt(&v))
            .transpose()?,
    })
}

fn map_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        checkpoint_id: row
            .get::<_, Option<String>>(2)?
            .map(parse_uuid)
            .transpose()?,
        kind: row.get(3)?,
        content: row.get(4)?,
        provenance_json: row.get(5)?,
        created_at: parse_dt(&row.get::<_, String>(6)?)?,
        updated_at: parse_dt(&row.get::<_, String>(7)?)?,
    })
}

fn map_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    Ok(Artifact {
        id: parse_uuid(row.get(0)?)?,
        task_id: parse_uuid(row.get(1)?)?,
        session_id: row
            .get::<_, Option<String>>(2)?
            .map(parse_uuid)
            .transpose()?,
        kind: row.get(3)?,
        name: row.get(4)?,
        local_path: row.get(5)?,
        content_type: row.get(6)?,
        size_bytes: row.get(7)?,
        checksum: row.get(8)?,
        metadata_json: row.get(9)?,
        created_at: parse_dt(&row.get::<_, String>(10)?)?,
    })
}

fn map_checkin(row: &rusqlite::Row<'_>) -> rusqlite::Result<CheckIn> {
    let kind_s: String = row.get(3)?;
    let status_s: String = row.get(4)?;
    Ok(CheckIn {
        id: parse_uuid(row.get::<_, String>(0)?)?,
        task_id: row
            .get::<_, Option<String>>(1)?
            .map(parse_uuid)
            .transpose()?,
        question: row.get(2)?,
        kind: CheckInKind::parse(&kind_s).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                format!("bad checkin kind {kind_s}").into(),
            )
        })?,
        status: CheckInStatus::parse(&status_s).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                format!("bad checkin status {status_s}").into(),
            )
        })?,
        answer_json: row.get(5)?,
        created_at: parse_dt(&row.get::<_, String>(6)?)?,
        answered_at: row
            .get::<_, Option<String>>(7)?
            .map(|s| parse_dt(&s))
            .transpose()?,
    })
}

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status_s: String = row.get(2)?;
    let source_s: String = row.get(3)?;
    Ok(Task {
        id: parse_uuid(row.get::<_, String>(0)?)?,
        title: row.get(1)?,
        status: TaskStatus::parse(&status_s).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                format!("bad status {status_s}").into(),
            )
        })?,
        source: TaskSource::parse(&source_s).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                format!("bad source {source_s}").into(),
            )
        })?,
        confidence: row.get(4)?,
        project: row.get(5)?,
        notes: row.get(6)?,
        suggested_next_action: row.get(7)?,
        dedup_key: row.get(8)?,
        source_session_id: row.get(9)?,
        sync_outcome: row
            .get::<_, Option<String>>(10)?
            .map(|value| {
                SyncOutcome::parse(&value)
                    .ok_or_else(|| bad_value(10, format!("bad sync outcome {value}")))
            })
            .transpose()?,
        sync_outcome_confidence: row.get(11)?,
        created_at: parse_dt(&row.get::<_, String>(12)?)?,
        updated_at: parse_dt(&row.get::<_, String>(13)?)?,
        completed_at: row
            .get::<_, Option<String>>(14)?
            .map(|s| parse_dt(&s))
            .transpose()?,
    })
}

fn map_sync_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncOutboxItem> {
    Ok(SyncOutboxItem {
        id: parse_uuid(row.get(0)?)?,
        record_type: row.get(1)?,
        record_id: parse_uuid(row.get(2)?)?,
        operation: row.get(3)?,
        payload_json: row.get(4)?,
        created_at: parse_dt(&row.get::<_, String>(5)?)?,
        attempt_count: row.get(6)?,
        next_attempt_at: parse_dt(&row.get::<_, String>(7)?)?,
        last_error: row.get(8)?,
    })
}

fn parse_uuid(s: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_dt(s: &str) -> rusqlite::Result<chrono::DateTime<Utc>> {
    DateTimeParse(s).try_into_dt()
}

fn validate_nonempty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(PulseError::Validation(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_json(value: &str, field: &str) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|e| PulseError::Validation(format!("invalid {field} JSON: {e}")))
}

fn parse_json_vec(value: &str) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(value).map_err(|e| bad_value(0, format!("invalid JSON string list: {e}")))
}

fn bad_value(column: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, message.into())
}

fn truncate_sync_error(error: &str) -> String {
    const MAX_LEN: usize = 1024;
    error.chars().take(MAX_LEN).collect()
}

struct DateTimeParse<'a>(&'a str);

impl DateTimeParse<'_> {
    fn try_into_dt(self) -> rusqlite::Result<chrono::DateTime<Utc>> {
        chrono::DateTime::parse_from_rfc3339(self.0)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(self.0, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| ndt.and_utc())
            })
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn store() -> Store {
        Store::new(db::open_in_memory().unwrap())
    }

    #[test]
    fn create_and_get_manual_task() {
        let s = store();
        let t = s.create_task(NewTask::manual("Ship pulse-core")).unwrap();
        assert_eq!(t.title, "Ship pulse-core");
        assert_eq!(t.status, TaskStatus::Inbox);
        assert_eq!(t.source, TaskSource::Manual);
        assert!(t.confidence.is_none());

        let got = s.get_task(t.id).unwrap().unwrap();
        assert_eq!(got.id, t.id);
    }

    #[test]
    fn persists_copilot_conversation_and_messages() {
        let s = store();
        let conversation = s
            .create_copilot_conversation("What should I work on today?")
            .unwrap();
        s.append_copilot_message(conversation.id, "user", "What should I work on today?", None, "[]")
            .unwrap();
        s.append_copilot_message(
            conversation.id,
            "assistant",
            "Focus on the most recently updated task.",
            Some("heuristic"),
            "[]",
        )
        .unwrap();

        let recent = s.list_recent_copilot_conversations(5).unwrap();
        assert_eq!(recent[0].id, conversation.id);
        let messages = s.list_copilot_messages(conversation.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].backend.as_deref(), Some("heuristic"));
    }

    #[test]
    fn session_sync_state_roundtrip_and_repoints_to_same_task() {
        let s = store();
        let task = s
            .create_task(NewTask::manual("Review imported session"))
            .unwrap();
        let now = Utc::now();
        let state = NewSessionSyncState {
            external_id: "codex:session-1".into(),
            source: "codex".into(),
            source_session_id: "session-1".into(),
            task_id: Some(task.id),
            content_fingerprint: "abc123".into(),
            source_mtime_ms: 10,
            source_size_bytes: 100,
            result: "created".into(),
            last_checked_at: now,
        };
        s.upsert_session_sync_state(state.clone()).unwrap();
        let mut updated = s
            .get_session_sync_state("codex:session-1")
            .unwrap()
            .unwrap();
        assert_eq!(updated.task_id, Some(task.id));
        assert_eq!(updated.content_fingerprint, "abc123");

        s.upsert_session_sync_state(NewSessionSyncState {
            content_fingerprint: "def456".into(),
            source_mtime_ms: 20,
            source_size_bytes: 200,
            result: "updated".into(),
            last_checked_at: now,
            ..state
        })
        .unwrap();
        updated = s
            .get_session_sync_state("codex:session-1")
            .unwrap()
            .unwrap();
        assert_eq!(updated.task_id, Some(task.id));
        assert_eq!(updated.content_fingerprint, "def456");
        assert_eq!(updated.source_mtime_ms, 20);
    }

    #[test]
    fn list_filters_by_status() {
        let s = store();
        let a = s.create_task(NewTask::manual("one")).unwrap();
        let b = s.create_task(NewTask::manual("two")).unwrap();
        s.mark_done(b.id).unwrap();

        let inbox = s.list_tasks(Some(TaskStatus::Inbox)).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, a.id);

        let done = s.list_tasks(Some(TaskStatus::Done)).unwrap();
        assert_eq!(done.len(), 1);
        assert!(done[0].completed_at.is_some());
    }

    #[test]
    fn mark_done_and_reopen() {
        let s = store();
        let t = s.create_task(NewTask::manual("finish me")).unwrap();
        let done = s.mark_done(t.id).unwrap();
        assert_eq!(done.status, TaskStatus::Done);
        assert!(done.completed_at.is_some());

        let reopened = s.set_status(t.id, TaskStatus::Inbox).unwrap();
        assert_eq!(reopened.status, TaskStatus::Inbox);
        assert!(reopened.completed_at.is_none());
    }

    #[test]
    fn updates_task_outcome() {
        let s = store();
        let task = s.create_task(NewTask::manual("Track task outcome")).unwrap();

        let updated = s
            .update_task(
                task.id,
                TaskUpdate {
                    sync_outcome: Some(SyncOutcome::Completed),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.sync_outcome, Some(SyncOutcome::Completed));
        assert_eq!(s.get_task(task.id).unwrap().unwrap().sync_outcome, Some(SyncOutcome::Completed));
    }

    #[test]
    fn rejects_invalid_transition() {
        let s = store();
        let t = s.create_task(NewTask::manual("done path")).unwrap();
        s.mark_done(t.id).unwrap();
        let err = s.set_status(t.id, TaskStatus::Today).unwrap_err();
        assert!(matches!(err, PulseError::InvalidTransition { .. }));
    }

    #[test]
    fn empty_title_rejected() {
        let s = store();
        assert!(s.create_task(NewTask::manual("   ")).is_err());
    }

    #[test]
    fn evidence_roundtrip() {
        let s = store();
        let t = s.create_task(NewTask::manual("with evidence")).unwrap();
        let ev = s
            .add_evidence(NewEvidence {
                task_id: t.id,
                kind: "manual".into(),
                source_ref: "user".into(),
                snippet: Some("because I said so".into()),
                metadata_json: None,
                observed_at: Utc::now(),
            })
            .unwrap();
        let list = s.list_evidence(t.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, ev.id);
        assert_eq!(list[0].snippet.as_deref(), Some("because I said so"));
    }

    #[test]
    fn dedup_key_lookup() {
        let s = store();
        let mut n = NewTask::manual("inferred-looking");
        n.source = TaskSource::Claude;
        n.dedup_key = Some("abc123".into());
        n.confidence = Some(0.4);
        let t = s.create_task(n).unwrap();
        let found = s.find_by_dedup_key("abc123").unwrap().unwrap();
        assert_eq!(found.id, t.id);
    }

    #[test]
    fn move_inbox_to_today() {
        let s = store();
        let t = s.create_task(NewTask::manual("focus")).unwrap();
        let updated = s.set_status(t.id, TaskStatus::Today).unwrap();
        assert_eq!(updated.status, TaskStatus::Today);
    }

    #[test]
    fn activity_timeline_records_roundtrip() {
        let s = store();
        let task = s
            .create_task(NewTask::manual("Implement activity timeline"))
            .unwrap();
        let now = Utc::now();
        let session = s
            .create_session(NewSession {
                task_id: task.id,
                agent: Some("codex".into()),
                application: Some("T3 Code".into()),
                repository_path: Some("D:/own/pulse".into()),
                external_id: Some("session-1".into()),
                source_ref: Some("codex:session-1".into()),
                started_at: now,
                ended_at: None,
                metadata_json: r#"{"branch":"main"}"#.into(),
            })
            .unwrap();
        let event = s
            .record_event(NewActivityEvent {
                task_id: task.id,
                session_id: Some(session.id),
                kind: "test_passed".into(),
                summary: "pulse-core tests passed".into(),
                payload_json: Some(r#"{"count":36}"#.into()),
                source_ref: None,
                occurred_at: now,
            })
            .unwrap();
        let checkpoint = s
            .create_checkpoint(NewCheckpoint {
                task_id: task.id,
                session_id: Some(session.id),
                summary: "Storage layer complete".into(),
                decisions: vec!["Keep tasks as activity roots".into()],
                failures: vec![],
                next_actions: vec!["Add IPC commands".into()],
                source_ref: None,
            })
            .unwrap();
        let reminder = s
            .create_reminder(NewReminder {
                task_id: task.id,
                title: "Review timeline UI".into(),
                due_at: now,
                context_json: r#"{"surface":"app"}"#.into(),
            })
            .unwrap();
        let memory = s
            .create_memory(NewMemory {
                task_id: task.id,
                checkpoint_id: Some(checkpoint.id),
                kind: "decision".into(),
                content: "Tasks remain the activity root.".into(),
                provenance_json: r#"{"checkpoint":true}"#.into(),
            })
            .unwrap();
        let artifact = s
            .create_artifact(NewArtifact {
                task_id: task.id,
                session_id: Some(session.id),
                kind: "patch".into(),
                name: "timeline.patch".into(),
                local_path: Some("D:/own/pulse/timeline.patch".into()),
                content_type: Some("text/x-diff".into()),
                size_bytes: Some(42),
                checksum: None,
                metadata_json: "{}".into(),
            })
            .unwrap();

        assert_eq!(s.list_sessions(task.id).unwrap()[0].id, session.id);
        assert_eq!(s.list_events(task.id).unwrap()[0].id, event.id);
        assert_eq!(
            s.list_checkpoints(task.id).unwrap()[0].decisions,
            ["Keep tasks as activity roots"]
        );
        assert_eq!(s.list_memories(task.id).unwrap()[0].id, memory.id);
        assert_eq!(s.list_artifacts(task.id).unwrap()[0].id, artifact.id);
        assert_eq!(s.list_due_reminders(now).unwrap()[0].id, reminder.id);

        let done = s
            .set_reminder_status(reminder.id, ReminderStatus::Done)
            .unwrap();
        assert_eq!(done.status, ReminderStatus::Done);
        assert!(done.completed_at.is_some());
        assert!(s.list_due_reminders(now).unwrap().is_empty());
    }

    #[test]
    fn timeline_rejects_session_from_another_task() {
        let s = store();
        let one = s.create_task(NewTask::manual("First task")).unwrap();
        let two = s.create_task(NewTask::manual("Second task")).unwrap();
        let session = s
            .create_session(NewSession::for_task(one.id, Utc::now()))
            .unwrap();
        let err = s
            .record_event(NewActivityEvent {
                task_id: two.id,
                session_id: Some(session.id),
                kind: "note".into(),
                summary: "incorrect session relationship".into(),
                payload_json: None,
                source_ref: None,
                occurred_at: Utc::now(),
            })
            .unwrap_err();
        assert!(matches!(err, PulseError::Validation(_)));
    }

    #[test]
    fn activity_changes_enter_the_durable_sync_outbox() {
        let store = store();
        let task = store
            .create_task(NewTask::manual("Queue this activity"))
            .unwrap();

        let pending = store.list_pending_sync(10, Utc::now()).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].record_type, "activity");
        assert_eq!(pending[0].record_id, task.id);
        assert_eq!(pending[0].operation, "upsert");

        store
            .mark_sync_delivered(&[pending[0].id], Utc::now())
            .unwrap();
        assert_eq!(store.pending_sync_count().unwrap(), 0);
    }
}
