use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, SourceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    Claude,
    Codex,
}

impl SourceId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredArtifact {
    pub path: PathBuf,
    /// Stable identity used as watermark key, e.g. `claude:projects/slug/session.jsonl`
    pub source_ref: String,
    pub project: Option<String>,
    pub session_id: String,
    pub size_bytes: u64,
    pub mtime_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ExtractedBatch {
    pub source_ref: String,
    pub path: PathBuf,
    pub project: Option<String>,
    pub session_id: String,
    /// New text since last watermark (complete lines only).
    pub candidate_text: String,
    /// Byte offset after last fully consumed line.
    pub new_byte_offset: u64,
    pub size_bytes: u64,
    pub mtime_ms: i64,
}

pub trait SourceAdapter: Send + Sync {
    fn id(&self) -> SourceId;
    fn discover(&self) -> Result<Vec<DiscoveredArtifact>>;
    fn extract(
        &self,
        artifact: &DiscoveredArtifact,
        since_offset: Option<u64>,
    ) -> Result<ExtractedBatch>;
}
