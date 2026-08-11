//! Work-signal source adapters (agent transcripts and local browser history).

pub mod brave;
pub mod claude;
pub mod codex;
pub mod extract;
pub mod types;

pub use brave::BraveSource;
pub use claude::ClaudeSource;
pub use codex::CodexSource;
pub use types::{DiscoveredArtifact, ExtractedBatch, SourceAdapter, SourceError, SourceId};
