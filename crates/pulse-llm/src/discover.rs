//! Resolve which agent CLI binary to use.

use std::path::{Path, PathBuf};

use pulse_core::config::LlmConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliBackendKind {
    Grok,
    Claude,
    Codex,
}

impl CliBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grok => "grok",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grok" => Some(Self::Grok),
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredBackend {
    pub kind: CliBackendKind,
    pub path: PathBuf,
}

/// Find binary for kind (config override or PATH).
pub fn find_backend(kind: CliBackendKind, cfg: &LlmConfig) -> Option<PathBuf> {
    let override_path = match kind {
        CliBackendKind::Grok => cfg.grok_bin.as_deref(),
        CliBackendKind::Claude => cfg.claude_bin.as_deref(),
        CliBackendKind::Codex => cfg.codex_bin.as_deref(),
    };
    if let Some(p) = override_path {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    find_on_path(kind.as_str())
}

/// Discover first available backend per preference / provider.
pub fn discover_cli_backend(cfg: &LlmConfig) -> Option<DiscoveredBackend> {
    let order: Vec<CliBackendKind> = if cfg.provider == "auto" {
        cfg.preference
            .iter()
            .filter_map(|s| CliBackendKind::parse(s))
            .collect()
    } else if let Some(k) = CliBackendKind::parse(&cfg.provider) {
        vec![k]
    } else {
        return None;
    };

    for kind in order {
        if let Some(path) = find_backend(kind, cfg) {
            return Some(DiscoveredBackend { kind, path });
        }
    }
    None
}

pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if let Some(p) = candidate_in_dir(&dir, name) {
            return Some(p);
        }
    }
    None
}

fn candidate_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        for ext in ["", ".exe", ".cmd", ".bat"] {
            let p = dir.join(format!("{name}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let p = dir.join(name);
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kinds() {
        assert_eq!(CliBackendKind::parse("grok"), Some(CliBackendKind::Grok));
        assert!(CliBackendKind::parse("openai").is_none());
    }
}
