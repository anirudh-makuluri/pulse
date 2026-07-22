use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("{0}")]
    Msg(String),
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
}

pub trait LlmClient: Send + Sync {
    fn backend_id(&self) -> &str;
    fn infer_tasks(&self, req: &InferRequest) -> Result<Vec<TaskCandidateOut>>;
}
