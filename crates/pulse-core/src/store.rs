use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{PulseError, Result};
use crate::models::{
    Evidence, NewEvidence, NewTask, Task, TaskSource, TaskStatus, TaskUpdate,
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

    pub fn create_task(&self, new: NewTask) -> Result<Task> {
        let title = new.title.trim();
        if title.is_empty() {
            return Err(PulseError::Validation("task title must not be empty".into()));
        }
        if let Some(c) = new.confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(PulseError::Validation(
                    "confidence must be in [0,1]".into(),
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
              created_at, updated_at, completed_at
            ) VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6, ?7,
              ?8, ?9, ?10,
              ?11, ?12, ?13
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
                now.to_rfc3339(),
                now.to_rfc3339(),
                completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

        self.get_task(id)?
            .ok_or_else(|| PulseError::TaskNotFound(id.to_string()))
    }

    pub fn get_task(&self, id: Uuid) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, title, status, source, confidence, project, notes,
                   suggested_next_action, dedup_key, source_session_id,
                   created_at, updated_at, completed_at
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
                       created_at, updated_at, completed_at
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
                       created_at, updated_at, completed_at
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
                return Err(PulseError::Validation("task title must not be empty".into()));
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
                return Err(PulseError::Validation(
                    "confidence must be in [0,1]".into(),
                ));
            }
            task.confidence = Some(c);
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
              updated_at = ?8,
              completed_at = ?9
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
                task.updated_at.to_rfc3339(),
                task.completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

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
                   created_at, updated_at, completed_at
            FROM tasks WHERE dedup_key = ?1
            "#,
        )?;
        let task = stmt
            .query_row(params![dedup_key], map_task)
            .optional()?;
        Ok(task)
    }
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
        created_at: parse_dt(&row.get::<_, String>(10)?)?,
        updated_at: parse_dt(&row.get::<_, String>(11)?)?,
        completed_at: row
            .get::<_, Option<String>>(12)?
            .map(|s| parse_dt(&s))
            .transpose()?,
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
}
