use thiserror::Error;

#[derive(Debug, Error)]
pub enum PulseError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("invalid status transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("schema version newer than binary supports (db={db}, binary={binary})")]
    SchemaTooNew { db: i64, binary: i64 },
}

pub type Result<T> = std::result::Result<T, PulseError>;
