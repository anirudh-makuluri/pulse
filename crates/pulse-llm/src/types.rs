use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("{0}")]
    Msg(String),
    #[error("backend failed: {0}")]
    Backend(String),
    #[error("timeout after {0}s")]
    Timeout(u64),
    #[error("parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, LlmError>;

#[derive(Debug, Clone)]
pub struct InferRequest {
    pub source: String,
    pub source_ref: String,
    pub session_id: String,
    pub project: Option<String>,
    /// Already redacted when remote; heuristic also accepts raw.
    pub candidate_text: String,
    pub max_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCandidateOut {
    pub title: String,
    pub notes: Option<String>,
    pub confidence: f64,
    pub suggested_next_action: Option<String>,
    pub proposed_status: Option<String>,
    pub evidence_snippet: Option<String>,
    /// If set, update this open task instead of creating.
    #[serde(default)]
    pub match_task_id: Option<String>,
    /// Required when a single LLM request contains multiple source sessions.
    #[serde(default)]
    pub source_session_id: Option<String>,
    /// LLM-observed session outcome; it is never a command to mark a task done.
    #[serde(default)]
    pub sync_outcome: Option<String>,
    #[serde(default)]
    pub sync_outcome_confidence: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SummaryRequest {
    pub day: String,
    pub task_lines: Vec<String>,
    pub activity_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryOut {
    pub text: String,
    pub highlights: Vec<String>,
}

pub trait LlmClient: Send + Sync {
    fn backend_id(&self) -> &str;
    fn infer_tasks(&self, req: &InferRequest) -> Result<Vec<TaskCandidateOut>>;
    fn summarize_day(&self, req: &SummaryRequest) -> Result<SummaryOut>;
}
