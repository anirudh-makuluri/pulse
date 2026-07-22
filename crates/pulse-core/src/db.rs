use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::error::{PulseError, Result};

/// Highest migration version this binary knows how to apply.
pub const LATEST_SCHEMA_VERSION: i64 = 1;

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");

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
        assert_eq!(v, 1);
    }

    #[test]
    fn open_file_db() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pulse.db");
        let conn = open(&path).unwrap();
        drop(conn);
        let conn2 = open(&path).unwrap();
        assert_eq!(current_version(&conn2).unwrap(), 1);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);
    }
}
