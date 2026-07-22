use std::path::PathBuf;

use walkdir::WalkDir;

use crate::claude::extract_jsonl_artifact;
use crate::extract::{mtime_ms, text_from_json_value};
use crate::types::{
    DiscoveredArtifact, ExtractedBatch, Result, SourceAdapter, SourceError, SourceId,
};

#[derive(Debug, Clone)]
pub struct CodexSource {
    pub root: PathBuf,
    pub extra_roots: Vec<PathBuf>,
    pub max_candidate_text_bytes: usize,
}

impl CodexSource {
    pub fn from_env(max_candidate_text_bytes: usize) -> Self {
        let root = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                directories::BaseDirs::new()
                    .map(|b| b.home_dir().join(".codex"))
                    .unwrap_or_else(|| PathBuf::from(".codex"))
            });
        Self {
            root,
            extra_roots: Vec::new(),
            max_candidate_text_bytes,
        }
    }

    pub fn with_root(root: impl Into<PathBuf>, max_candidate_text_bytes: usize) -> Self {
        Self {
            root: root.into(),
            extra_roots: Vec::new(),
            max_candidate_text_bytes,
        }
    }

    fn sessions_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.root.join("sessions")];
        for r in &self.extra_roots {
            dirs.push(r.join("sessions"));
        }
        dirs
    }

    /// Best-effort cwd from first session_meta / turn_context lines.
    fn probe_project(path: &std::path::Path) -> Option<String> {
        use std::io::{BufRead, BufReader};
        let file = std::fs::File::open(path).ok()?;
        let reader = BufReader::new(file);
        for (i, line) in reader.lines().enumerate() {
            if i > 40 {
                break;
            }
            let line = line.ok()?;
            let v: serde_json::Value = serde_json::from_str(&line).ok()?;
            if let Some(cwd) = v
                .pointer("/payload/cwd")
                .and_then(|c| c.as_str())
                .or_else(|| v.get("cwd").and_then(|c| c.as_str()))
            {
                // Use last path component as project key
                let p = std::path::Path::new(cwd);
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    return Some(name.to_string());
                }
                return Some(cwd.to_string());
            }
            let _ = text_from_json_value(&v);
        }
        None
    }
}

impl SourceAdapter for CodexSource {
    fn id(&self) -> SourceId {
        SourceId::Codex
    }

    fn discover(&self) -> Result<Vec<DiscoveredArtifact>> {
        let mut out = Vec::new();
        for sessions in self.sessions_dirs() {
            if !sessions.is_dir() {
                continue;
            }
            for entry in WalkDir::new(&sessions).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
                    continue;
                }
                let rel = path
                    .strip_prefix(&sessions)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let session_id = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel.clone());
                let project = Self::probe_project(path);
                let meta = std::fs::metadata(path)?;
                out.push(DiscoveredArtifact {
                    path: path.to_path_buf(),
                    source_ref: format!("codex:sessions/{rel}"),
                    project,
                    session_id,
                    size_bytes: meta.len(),
                    mtime_ms: mtime_ms(path)?,
                });
            }
        }
        Ok(out)
    }

    fn extract(
        &self,
        artifact: &DiscoveredArtifact,
        since_offset: Option<u64>,
    ) -> Result<ExtractedBatch> {
        let mut batch =
            extract_jsonl_artifact(artifact, since_offset, self.max_candidate_text_bytes)?;
        // Refresh project from file if still unknown.
        if batch.project.is_none() {
            batch.project = Self::probe_project(&artifact.path);
        }
        Ok(batch)
    }
}

// silence unused warning if SourceError only used via Result
#[allow(dead_code)]
fn _err() -> SourceError {
    SourceError::Msg("x".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn discovers_rollout_jsonl() {
        let dir = tempdir().unwrap();
        let day = dir.path().join("sessions").join("2026").join("03").join("27");
        std::fs::create_dir_all(&day).unwrap();
        let sess = day.join("rollout-2026-03-27T12-00-00-abcd.jsonl");
        let mut f = std::fs::File::create(&sess).unwrap();
        writeln!(
            f,
            "{}",
            r#"{"type":"session_meta","payload":{"cwd":"C:\\work\\demo-app","id":"x"}}"#
        )
        .unwrap();
        writeln!(
            f,
            "{}",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Please fix the broken export button"}]}}"#
        )
        .unwrap();

        let src = CodexSource::with_root(dir.path(), 65_536);
        let found = src.discover().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].project.as_deref(), Some("demo-app"));

        let batch = src.extract(&found[0], Some(0)).unwrap();
        assert!(batch.candidate_text.to_lowercase().contains("export"));
    }
}
