//! Inference backends: heuristic (PR4) and agent CLIs (PR5).

pub mod heuristic;
pub mod sanitize;
pub mod types;

pub use heuristic::HeuristicClient;
pub use sanitize::redact_for_remote;
pub use types::{InferRequest, LlmClient, LlmError, TaskCandidateOut};
