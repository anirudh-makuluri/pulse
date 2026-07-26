//! Local Hugging Face embedding provider backed by ONNX Runtime.
//!
//! The selected model is downloaded to the Pulse data directory once, then all
//! text embedding happens on-device. No Hugging Face API token is required.

use std::path::PathBuf;

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use pulse_core::{EmbeddingsConfig, PulsePaths};

use crate::types::{LlmError, Result};

pub struct HuggingFaceEmbeddingClient {
    model: TextEmbedding,
    dimensions: usize,
}

impl HuggingFaceEmbeddingClient {
    pub fn from_config(config: &EmbeddingsConfig) -> Result<Self> {
        if config.provider != "huggingface_onnx" {
            return Err(LlmError::Msg(format!(
                "embedding provider {} is not huggingface_onnx",
                config.provider
            )));
        }
        if config.model != "sentence-transformers/all-MiniLM-L6-v2" {
            return Err(LlmError::Msg(format!(
                "unsupported local Hugging Face embedding model {}",
                config.model
            )));
        }
        if config.dimensions != 384 {
            return Err(LlmError::Msg(format!(
                "{} produces 384-dimensional vectors, but config requires {}",
                config.model, config.dimensions
            )));
        }

        let cache_dir = config
            .cache_dir
            .as_deref()
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(default_cache_dir)?;
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| LlmError::Backend(format!("create embedding cache: {e}")))?;
        let options = TextInitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(false);
        let model = TextEmbedding::try_new(options)
            .map_err(|e| LlmError::Backend(format!("load local MiniLM model: {e}")))?;
        Ok(Self {
            model,
            dimensions: 384,
        })
    }

    pub fn embed(&mut self, input: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let vectors = self
            .model
            .embed(input, None)
            .map_err(|e| LlmError::Backend(format!("generate MiniLM embeddings: {e}")))?;
        if vectors.iter().any(|vector| vector.len() != self.dimensions) {
            return Err(LlmError::Parse(format!(
                "MiniLM returned a vector with dimensions other than {}",
                self.dimensions
            )));
        }
        Ok(vectors)
    }
}

fn default_cache_dir() -> Result<PathBuf> {
    PulsePaths::default()
        .map(|paths| paths.root.join("models"))
        .map_err(|e| LlmError::Backend(format!("resolve embedding cache: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_default_minilm_config() {
        let config = EmbeddingsConfig::default();
        assert_eq!(config.provider, "huggingface_onnx");
        assert_eq!(config.model, "sentence-transformers/all-MiniLM-L6-v2");
        assert_eq!(config.dimensions, 384);
    }
}
