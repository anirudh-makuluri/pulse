use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::error::{PulseError, Result};

/// Highest migration version this binary knows how to apply.
pub const LATEST_SCHEMA_VERSION: i64 = 7;

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../migrations/002_activity_timeline.sql");
const MIGRATION_003: &str = include_str!("../migrations/003_sync_outbox.sql");
const MIGRATION_004: &str = include_str!("../migrations/004_sync_outcome.sql");
const MIGRATION_005: &str = include_str!("../migrations/005_session_sync_state.sql");
const MIGRATION_006: &str = include_str!("../migrations/006_copilot_conversations.sql");
const MIGRATION_007: &str = include_str!("../migrations/007_brave_source.sql");

/// Open (or create) the SQLite database, enable pragmas, apply migrations.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database (tests).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        "#,
    )?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<()> {
    let current = current_version(conn)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(PulseError::SchemaTooNew {
            db: current,
            binary: LATEST_SCHEMA_VERSION,
        });
    }
    if current < 1 {
        conn.execute_batch(MIGRATION_001)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [1i64],
        )?;
    }
    if current < 2 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATION_002)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [2i64],
        )?;
        tx.commit()?;
    }
    if current < 3 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATION_003)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [3i64],
        )?;
        tx.commit()?;
    }
    if current < 4 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATION_004)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [4i64],
        )?;
        tx.commit()?;
    }
    if current < 5 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATION_005)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [5i64],
        )?;
        tx.commit()?;
    }
    if current < 6 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATION_006)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
            [6i64],
        )?;
        tx.commit()?;
    }
    if current < 7 {
        // SQLite cannot alter CHECK constraints. This migration rebuilds the
        // two constrained tables while foreign keys are temporarily disabled.
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let migration = (|| -> Result<()> {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(MIGRATION_007)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
                [7i64],
            )?;
            tx.commit()?;
            Ok(())
        })();
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migration?;
    }
    Ok(())
}

fn current_version(conn: &Connection) -> Result<i64> {
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);

    if !table_exists {
        return Ok(0);
    }

    let version: Option<i64> = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(version.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrate_creates_tasks_table() {
        let conn = open_in_memory().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        let v = current_version(&conn).unwrap();
        assert_eq!(v, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn open_file_db() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pulse.db");
        let conn = open(&path).unwrap();
        drop(conn);
        let conn2 = open(&path).unwrap();
        assert_eq!(current_version(&conn2).unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_creates_activity_timeline_tables() {
        let conn = open_in_memory().unwrap();
        for table in [
            "sessions",
            "events",
            "checkpoints",
            "reminders",
            "memories",
            "artifacts",
            "sync_outbox",
            "session_sync_state",
            "copilot_conversations",
            "copilot_messages",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing {table} table");
        }
    }

    #[test]
    fn migrates_existing_v1_database_to_activity_timeline() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        conn.execute_batch(MIGRATION_001).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (1, datetime('now'))",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        assert_eq!(current_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
        let count: i64 = conn
            .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('sessions', 'events', 'checkpoints', 'reminders', 'memories', 'artifacts', 'sync_outbox', 'session_sync_state')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 8);
    }

    #[test]
    fn migrates_existing_tasks_to_the_brave_source_schema() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        for migration in [
            MIGRATION_001,
            MIGRATION_002,
            MIGRATION_003,
            MIGRATION_004,
            MIGRATION_005,
            MIGRATION_006,
        ] {
            conn.execute_batch(migration).unwrap();
        }
        for version in 1..=6 {
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
                [version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO tasks (
              id, title, status, source, confidence, project, notes,
              suggested_next_action, dedup_key, source_session_id,
              created_at, updated_at, completed_at, sync_outcome, sync_outcome_confidence
            ) VALUES (
              'task-1', 'Keep the existing task', 'Inbox', 'codex', NULL, NULL, NULL,
              NULL, NULL, 'session-1',
              '2026-08-05T00:00:00Z', '2026-08-05T01:00:00Z', NULL, 'in_progress', 0.8
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO evidence (id, task_id, kind, source_ref, snippet, metadata_json, observed_at)
             VALUES ('evidence-1', 'task-1', 'session_snippet', 'codex:session-1', NULL, NULL, '2026-08-05T01:00:00Z')",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let row: (String, String, String) = conn
            .query_row(
                "SELECT source, updated_at, sync_outcome FROM tasks WHERE id = 'task-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, "codex");
        assert_eq!(row.1, "2026-08-05T01:00:00Z");
        assert_eq!(row.2, "in_progress");
        let foreign_key_errors: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_key_errors, 0);
    }
}
