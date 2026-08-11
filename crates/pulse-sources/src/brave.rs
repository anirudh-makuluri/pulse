use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};

use crate::extract::mtime_ms;
use crate::types::{DiscoveredArtifact, ExtractedBatch, Result, SourceAdapter, SourceId};

/// Read recent, local Brave history as a deliberately opt-in work signal.
/// Pulse opens the History database read-only and never changes the browser
/// profile or copies its cookies, saved passwords, or other profile data.
#[derive(Debug, Clone)]
pub struct BraveSource {
    pub root: PathBuf,
    pub extra_roots: Vec<PathBuf>,
    pub max_candidate_text_bytes: usize,
}

impl BraveSource {
    pub fn from_env(max_candidate_text_bytes: usize) -> Self {
        let root = std::env::var_os("BRAVE_USER_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|v| PathBuf::from(v).join("BraveSoftware").join("Brave-Browser").join("User Data")))
            .unwrap_or_else(|| PathBuf::from("BraveSoftware/Brave-Browser/User Data"));
        Self { root, extra_roots: Vec::new(), max_candidate_text_bytes }
    }

    pub fn with_root(root: impl Into<PathBuf>, max_candidate_text_bytes: usize) -> Self {
        Self { root: root.into(), extra_roots: Vec::new(), max_candidate_text_bytes }
    }

    fn history_files_under(root: &Path) -> Vec<(String, PathBuf)> {
        if root.file_name().and_then(|name| name.to_str()) == Some("History") && root.is_file() {
            return vec![("custom".into(), root.to_path_buf())];
        }

        let mut profiles = Vec::new();
        for name in ["Default"] {
            let history = root.join(name).join("History");
            if history.is_file() {
                profiles.push((name.into(), history));
            }
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("Profile ") {
                    continue;
                }
                let history = entry.path().join("History");
                if history.is_file() {
                    profiles.push((name, history));
                }
            }
        }
        profiles
    }

    fn history_files(&self) -> Vec<(String, PathBuf)> {
        let mut roots = vec![self.root.clone()];
        roots.extend(self.extra_roots.iter().cloned());
        roots.into_iter()
            .flat_map(|root| Self::history_files_under(&root))
            .collect()
    }
}

impl SourceAdapter for BraveSource {
    fn id(&self) -> SourceId {
        SourceId::Brave
    }

    fn discover(&self) -> Result<Vec<DiscoveredArtifact>> {
        let mut artifacts = Vec::new();
        for (profile, path) in self.history_files() {
            let metadata = fs::metadata(&path)?;
            let modified_at = mtime_ms(&path)?;
            artifacts.push(DiscoveredArtifact {
                path,
                source_ref: format!("brave:history/{profile}"),
                project: None,
                session_id: profile,
                size_bytes: metadata.len(),
                mtime_ms: modified_at,
            });
        }
        Ok(artifacts)
    }

    fn extract(&self, artifact: &DiscoveredArtifact, since_offset: Option<u64>) -> Result<ExtractedBatch> {
        let since = since_offset.unwrap_or(0) as i64;
        let connection = Connection::open_with_flags(
            &artifact.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut statement = connection.prepare(
            "SELECT url, title, last_visit_time
             FROM urls
             WHERE last_visit_time > ?1 AND url NOT LIKE 'brave://%'
             ORDER BY last_visit_time ASC
             LIMIT 100",
        )?;
        let mut rows = statement.query([since])?;
        let mut text = String::new();
        let mut new_offset = since as u64;
        while let Some(row) = rows.next()? {
            let url: String = row.get(0)?;
            let title: String = row.get(1)?;
            let visited_at: i64 = row.get(2)?;
            let unix_seconds = visited_at / 1_000_000 - 11_644_473_600;
            let timestamp = Utc
                .timestamp_opt(unix_seconds, ((visited_at % 1_000_000).max(0) as u32) * 1_000)
                .single()
                .map(|time| time.to_rfc3339())
                .unwrap_or_else(|| "unknown time".into());
            let line = format!("[{timestamp}] {title}\n{url}\n");
            if text.len() + line.len() > self.max_candidate_text_bytes {
                break;
            }
            text.push_str(&line);
            new_offset = new_offset.max(visited_at.max(0) as u64);
        }

        Ok(ExtractedBatch {
            source_ref: artifact.source_ref.clone(),
            path: artifact.path.clone(),
            project: None,
            session_id: artifact.session_id.clone(),
            candidate_text: text,
            new_byte_offset: new_offset,
            size_bytes: artifact.size_bytes,
            mtime_ms: artifact.mtime_ms,
        })
    }

    fn watermark_offset_is_byte_position(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn reads_recent_history_without_writing_to_it() {
        let dir = tempdir().unwrap();
        let profile = dir.path().join("Default");
        fs::create_dir_all(&profile).unwrap();
        let history = profile.join("History");
        let connection = Connection::open(&history).unwrap();
        connection.execute_batch("CREATE TABLE urls (url TEXT, title TEXT, last_visit_time INTEGER);").unwrap();
        connection.execute(
            "INSERT INTO urls (url, title, last_visit_time) VALUES (?1, ?2, ?3)",
            params!["https://example.test/planning", "Plan the next release", 13_400_000_000_000_000i64],
        ).unwrap();
        drop(connection);

        let source = BraveSource::with_root(dir.path(), 65_536);
        let artifact = source.discover().unwrap().pop().unwrap();
        let batch = source.extract(&artifact, Some(0)).unwrap();
        assert!(batch.candidate_text.contains("Plan the next release"));
        assert!(batch.candidate_text.contains("https://example.test/planning"));
        assert!(batch.new_byte_offset > 0);
    }
}
