//! Inference backends: heuristic + agent CLIs (grok/claude/codex).

pub mod cli_backend;
pub mod discover;
pub mod heuristic;
pub mod parse;
pub mod resolve;
pub mod sanitize;
pub mod types;

pub use cli_backend::CliLlmClient;
pub use discover::{discover_cli_backend, CliBackendKind};
pub use heuristic::HeuristicClient;
pub use resolve::{llm_status, probe_preference, resolve_llm_client, LlmStatus};
pub use sanitize::redact_for_remote;
pub use types::{
    InferRequest, LlmClient, LlmError, SummaryOut, SummaryRequest, TaskCandidateOut,
};
