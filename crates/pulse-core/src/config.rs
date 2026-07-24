use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub service: ServiceConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
    #[serde(default)]
    pub sources: SourcesConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            service: ServiceConfig::default(),
            llm: LlmConfig::default(),
            inference: InferenceConfig::default(),
            sources: SourcesConfig::default(),
            sync: SyncConfig::default(),
            privacy: PrivacyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceConfig {
    #[serde(default = "default_pipe_name")]
    pub pipe_name: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_max_files")]
    pub log_max_files: u32,
    #[serde(default = "default_log_max_bytes")]
    pub log_max_bytes: u64,
}

fn default_pipe_name() -> String {
    "pulse-service".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_max_files() -> u32 {
    7
}
fn default_log_max_bytes() -> u64 {
    10_485_760
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            pipe_name: default_pipe_name(),
            log_level: default_log_level(),
            log_max_files: default_log_max_files(),
            log_max_bytes: default_log_max_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmConfig {
    /// "auto" | "grok" | "claude" | "codex" | "none"
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    #[serde(default = "default_llm_preference")]
    pub preference: Vec<String>,
    #[serde(default = "default_llm_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_concurrent_llm")]
    pub max_concurrent_llm_calls: u32,
    #[serde(default)]
    pub grok_bin: Option<String>,
    #[serde(default)]
    pub claude_bin: Option<String>,
    #[serde(default)]
    pub codex_bin: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

fn default_llm_provider() -> String {
    "auto".into()
}
fn default_llm_preference() -> Vec<String> {
    vec!["grok".into(), "claude".into(), "codex".into()]
}
fn default_llm_timeout() -> u64 {
    120
}
fn default_max_concurrent_llm() -> u32 {
    1
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            preference: default_llm_preference(),
            timeout_secs: default_llm_timeout(),
            max_concurrent_llm_calls: default_max_concurrent_llm(),
            grok_bin: None,
            claude_bin: None,
            codex_bin: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_checkin_threshold")]
    pub checkin_threshold: f64,
    #[serde(default = "default_auto_status_threshold")]
    pub auto_status_threshold: f64,
    #[serde(default = "default_strong_done_threshold")]
    pub strong_done_threshold: f64,
    #[serde(default = "default_dedup_title_similarity")]
    pub dedup_title_similarity: f64,
    #[serde(default = "default_max_candidate_text_bytes")]
    pub max_candidate_text_bytes: u64,
    #[serde(default = "default_max_candidates_per_batch")]
    pub max_candidates_per_batch: u32,
    #[serde(default = "default_heuristic_inbox_inserts")]
    pub heuristic_inbox_inserts_per_hour: u32,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_max_queued_jobs")]
    pub max_queued_jobs: u32,
}

fn default_true() -> bool {
    true
}
fn default_checkin_threshold() -> f64 {
    0.55
}
fn default_auto_status_threshold() -> f64 {
    0.75
}
fn default_strong_done_threshold() -> f64 {
    0.90
}
fn default_dedup_title_similarity() -> f64 {
    0.92
}
fn default_max_candidate_text_bytes() -> u64 {
    65_536
}
fn default_max_candidates_per_batch() -> u32 {
    5
}
fn default_heuristic_inbox_inserts() -> u32 {
    20
}
fn default_debounce_ms() -> u64 {
    2000
}
fn default_max_queued_jobs() -> u32 {
    32
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkin_threshold: default_checkin_threshold(),
            auto_status_threshold: default_auto_status_threshold(),
            strong_done_threshold: default_strong_done_threshold(),
            dedup_title_similarity: default_dedup_title_similarity(),
            max_candidate_text_bytes: default_max_candidate_text_bytes(),
            max_candidates_per_batch: default_max_candidates_per_batch(),
            heuristic_inbox_inserts_per_hour: default_heuristic_inbox_inserts(),
            debounce_ms: default_debounce_ms(),
            max_queued_jobs: default_max_queued_jobs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SourcesConfig {
    #[serde(default)]
    pub claude: SourceToggle,
    #[serde(default)]
    pub codex: SourceToggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceToggle {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub extra_roots: Vec<String>,
}

/// Optional cloud synchronization. The local SQLite database remains usable
/// while sync is disabled or the endpoint is unreachable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    /// Explicit opt-in. Disabled by default to preserve local-first behavior.
    #[serde(default)]
    pub enabled: bool,
    /// HTTPS endpoint for the Pulse sync API. Required when sync is enabled.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Optional S3 bucket or equivalent artifact store identifier. This stores
    /// only explicitly approved large artifacts; it never enables sync itself.
    #[serde(default)]
    pub artifact_bucket: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            artifact_bucket: None,
        }
    }
}

impl Default for SourceToggle {
    fn default() -> Self {
        Self {
            enabled: false,
            extra_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub acknowledge_remote_llm: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            acknowledge_remote_llm: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let p = self.llm.provider.as_str();
        if !matches!(p, "auto" | "grok" | "claude" | "codex" | "none") {
            return Err(PulseError::Config(format!(
                "invalid llm.provider '{p}' (expected auto|grok|claude|codex|none)"
            )));
        }
        for name in &self.llm.preference {
            if !matches!(name.as_str(), "grok" | "claude" | "codex") {
                return Err(PulseError::Config(format!(
                    "invalid llm.preference entry '{name}'"
                )));
            }
        }
        if self.llm.timeout_secs == 0 {
            return Err(PulseError::Config("llm.timeout_secs must be > 0".into()));
        }
        if self.llm.max_concurrent_llm_calls == 0 {
            return Err(PulseError::Config(
                "llm.max_concurrent_llm_calls must be > 0".into(),
            ));
        }
        for (name, v) in [
            ("checkin_threshold", self.inference.checkin_threshold),
            (
                "auto_status_threshold",
                self.inference.auto_status_threshold,
            ),
            (
                "strong_done_threshold",
                self.inference.strong_done_threshold,
            ),
            (
                "dedup_title_similarity",
                self.inference.dedup_title_similarity,
            ),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(PulseError::Config(format!(
                    "inference.{name} must be in [0,1], got {v}"
                )));
            }
        }
        if self.inference.auto_status_threshold < self.inference.checkin_threshold {
            return Err(PulseError::Config(
                "auto_status_threshold must be >= checkin_threshold".into(),
            ));
        }
        if self.inference.strong_done_threshold < self.inference.auto_status_threshold {
            return Err(PulseError::Config(
                "strong_done_threshold must be >= auto_status_threshold".into(),
            ));
        }
        if self.service.pipe_name.trim().is_empty() {
            return Err(PulseError::Config("service.pipe_name must not be empty".into()));
        }
        if self.sync.enabled && self.sync.endpoint.is_none() {
            return Err(PulseError::Config(
                "sync.endpoint is required when sync.enabled is true".into(),
            ));
        }
        if let Some(endpoint) = &self.sync.endpoint {
            let endpoint = endpoint.trim();
            if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
                return Err(PulseError::Config(
                    "sync.endpoint must start with https:// or http://".into(),
                ));
            }
        }
        if self
            .sync
            .artifact_bucket
            .as_deref()
            .is_some_and(|bucket| bucket.trim().is_empty())
        {
            return Err(PulseError::Config(
                "sync.artifact_bucket must not be empty when provided".into(),
            ));
        }
        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| PulseError::Config(format!("serialize config: {e}")))
    }
}

/// Load config from `path`. If missing, write defaults and return them.
pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        let cfg = Config::default();
        cfg.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(path, &cfg)?;
        return Ok(cfg);
    }
    let text = fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&text)
        .map_err(|e| PulseError::Config(format!("parse {}: {e}", path.display())))?;
    cfg.validate()?;
    Ok(cfg)
}

/// Parse config from a TOML string (no I/O).
pub fn parse_str(text: &str) -> Result<Config> {
    let cfg: Config = toml::from_str(text).map_err(|e| PulseError::Config(e.to_string()))?;
    cfg.validate()?;
    Ok(cfg)
}

/// Atomically write config (temp + rename).
pub fn write_atomic(path: &Path, cfg: &Config) -> Result<()> {
    cfg.validate()?;
    let text = cfg.to_toml_string()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("config.toml")
    ));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_config_validates() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn default_preference_order() {
        let cfg = Config::default();
        assert_eq!(cfg.llm.preference, vec!["grok", "claude", "codex"]);
        assert_eq!(cfg.llm.provider, "auto");
        assert!(!cfg.privacy.acknowledge_remote_llm);
        assert!(!cfg.sources.claude.enabled);
        assert!(!cfg.sync.enabled);
    }

    #[test]
    fn load_creates_default_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = load(&path).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.llm.provider, "auto");
        let again = load(&path).unwrap();
        assert_eq!(cfg, again);
    }

    #[test]
    fn rejects_bad_provider() {
        let err = parse_str(
            r#"
[llm]
provider = "openai"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("provider"));
    }

    #[test]
    fn rejects_threshold_order() {
        let err = parse_str(
            r#"
[inference]
checkin_threshold = 0.9
auto_status_threshold = 0.5
strong_done_threshold = 0.95
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("auto_status_threshold"));
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let cfg = parse_str(
            r#"
[service]
log_level = "debug"
"#,
        )
        .unwrap();
        assert_eq!(cfg.service.log_level, "debug");
        assert_eq!(cfg.service.pipe_name, "pulse-service");
        assert_eq!(cfg.inference.debounce_ms, 2000);
    }

    #[test]
    fn sync_requires_an_endpoint_when_enabled() {
        let err = parse_str(
            r#"
[sync]
enabled = true
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("sync.endpoint"));
    }

    #[test]
    fn sync_config_accepts_a_local_development_endpoint() {
        let cfg = parse_str(
            r#"
[sync]
enabled = true
endpoint = "http://127.0.0.1:3000"
artifact_bucket = "pulse-artifacts"
"#,
        )
        .unwrap();
        assert!(cfg.sync.enabled);
        assert_eq!(cfg.sync.artifact_bucket.as_deref(), Some("pulse-artifacts"));
    }
}
