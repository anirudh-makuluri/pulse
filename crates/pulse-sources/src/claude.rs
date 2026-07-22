use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::extract::{extract_text_from_jsonl_chunk, mtime_ms, read_complete_lines};
use crate::types::{
    DiscoveredArtifact, ExtractedBatch, Result, SourceAdapter, SourceId,
};

#[derive(Debug, Clone)]
pub struct ClaudeSource {
    pub root: PathBuf,
    pub extra_roots: Vec<PathBuf>,
    pub max_candidate_text_bytes: usize,
}

impl ClaudeSource {
    pub fn from_env(max_candidate_text_bytes: usize) -> Self {
        let root = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                directories::BaseDirs::new()
                    .map(|b| b.home_dir().join(".claude"))
                    .unwrap_or_else(|| PathBuf::from(".claude"))
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

    fn projects_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.root.join("projects")];
        for r in &self.extra_roots {
            dirs.push(r.join("projects"));
        }
        dirs
    }
}

impl SourceAdapter for ClaudeSource {
    fn id(&self) -> SourceId {
        SourceId::Claude
    }

    fn discover(&self) -> Result<Vec<DiscoveredArtifact>> {
        let mut out = Vec::new();
        for projects in self.projects_dirs() {
            if !projects.is_dir() {
                continue;
            }
            for entry in WalkDir::new(&projects).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&projects)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let project = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string());
                let session_id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel.clone());
                let meta = std::fs::metadata(path)?;
                let source_ref = format!("claude:projects/{rel}");
                out.push(DiscoveredArtifact {
                    path: path.to_path_buf(),
                    source_ref,
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
        extract_jsonl_artifact(artifact, since_offset, self.max_candidate_text_bytes)
    }
}

pub(crate) fn extract_jsonl_artifact(
    artifact: &DiscoveredArtifact,
    since_offset: Option<u64>,
    max_bytes: usize,
) -> Result<ExtractedBatch> {
    let (chunk, new_offset, size) = read_complete_lines(&artifact.path, since_offset)?;
    // Shrink detection: caller should pass 0 if size < watermark size.
    let text = extract_text_from_jsonl_chunk(&chunk, max_bytes);
    Ok(ExtractedBatch {
        source_ref: artifact.source_ref.clone(),
        path: artifact.path.clone(),
        project: artifact.project.clone(),
        session_id: artifact.session_id.clone(),
        candidate_text: text,
        new_byte_offset: new_offset,
        size_bytes: size,
        mtime_ms: mtime_ms(&artifact.path).unwrap_or(artifact.mtime_ms),
    })
}

/// Allow tests / pipeline to force a specific root without env.
pub fn discover_under(root: &Path, max_bytes: usize) -> Result<Vec<DiscoveredArtifact>> {
    ClaudeSource::with_root(root, max_bytes).discover()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn discovers_project_jsonl() {
        let dir = tempdir().unwrap();
        let proj = dir.path().join("projects").join("my-app");
        std::fs::create_dir_all(&proj).unwrap();
        let sess = proj.join("abc-session.jsonl");
        let mut f = std::fs::File::create(&sess).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"Please add login form validation"}}}}"#
        )
        .unwrap();

        let src = ClaudeSource::with_root(dir.path(), 65_536);
        let found = src.discover().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].project.as_deref(), Some("my-app"));
        assert_eq!(found[0].session_id, "abc-session");

        let batch = src.extract(&found[0], Some(0)).unwrap();
        assert!(batch.candidate_text.to_lowercase().contains("login"));
        assert!(batch.new_byte_offset > 0);
    }
}
