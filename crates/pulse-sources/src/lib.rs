//! Work-signal source adapters (Claude / Codex session files).

pub mod claude;
pub mod codex;
pub mod extract;
pub mod types;

pub use claude::ClaudeSource;
pub use codex::CodexSource;
pub use types::{DiscoveredArtifact, ExtractedBatch, SourceAdapter, SourceError, SourceId};
