//! Export task history to JSON or Markdown.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{PulseError, Result};
use crate::models::{Evidence, Task};
use crate::paths::PulsePaths;
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}

#[derive(Debug, Serialize)]
struct ExportTask {
    #[serde(flatten)]
    task: Task,
    evidence: Vec<Evidence>,
}

#[derive(Debug, Serialize)]
struct ExportDoc {
    exported_at: String,
    from: Option<String>,
    to: Option<String>,
    task_count: usize,
    tasks: Vec<ExportTask>,
}

/// Export tasks (optionally filtered by updated_at day range) to `out` or default exports dir.
pub fn export_history(
    store: &Store,
    paths: &PulsePaths,
    format: ExportFormat,
    from: Option<&str>,
    to: Option<&str>,
    out: Option<&Path>,
) -> Result<PathBuf> {
    paths.ensure_layout()?;
    let mut tasks = store.list_tasks(None)?;
    if from.is_some() || to.is_some() {
        tasks.retain(|t| within_range(t.updated_at, from, to));
    }

    let mut rows = Vec::with_capacity(tasks.len());
    for task in tasks {
        let evidence = store.list_evidence(task.id)?;
        rows.push(ExportTask { task, evidence });
    }

    let doc = ExportDoc {
        exported_at: Utc::now().to_rfc3339(),
        from: from.map(|s| s.to_string()),
        to: to.map(|s| s.to_string()),
        task_count: rows.len(),
        tasks: rows,
    };

    let path = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let stamp = Utc::now().format("%Y%m%d-%H%M%S");
            paths
                .exports_dir()
                .join(format!("pulse-history-{stamp}.{}", format.extension()))
        }
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    match format {
        ExportFormat::Json => {
            let text = serde_json::to_string_pretty(&doc)
                .map_err(|e| PulseError::Validation(format!("json encode: {e}")))?;
            fs::write(&path, text)?;
        }
        ExportFormat::Markdown => {
            fs::write(&path, render_markdown(&doc))?;
        }
    }
    Ok(path)
}

fn within_range(ts: DateTime<Utc>, from: Option<&str>, to: Option<&str>) -> bool {
    let day = ts.format("%Y-%m-%d").to_string();
    if let Some(f) = from {
        if day.as_str() < f {
            return false;
        }
    }
    if let Some(t) = to {
        if day.as_str() > t {
            return false;
        }
    }
    true
}

fn render_markdown(doc: &ExportDoc) -> String {
    let mut out = String::new();
    out.push_str("# Pulse export\n\n");
    out.push_str(&format!("Exported: {}\n\n", doc.exported_at));
    if doc.from.is_some() || doc.to.is_some() {
        out.push_str(&format!(
            "Range: {} → {}\n\n",
            doc.from.as_deref().unwrap_or("…"),
            doc.to.as_deref().unwrap_or("…")
        ));
    }
    out.push_str(&format!("Tasks: {}\n\n", doc.task_count));
    for row in &doc.tasks {
        let t = &row.task;
        out.push_str(&format!("## {}\n\n", t.title));
        out.push_str(&format!(
            "- **id:** `{}`\n- **status:** {}\n- **source:** {}\n- **updated:** {}\n",
            t.id,
            t.status,
            t.source,
            t.updated_at.to_rfc3339()
        ));
        if let Some(c) = t.confidence {
            out.push_str(&format!("- **confidence:** {c:.2}\n"));
        }
        if let Some(p) = &t.project {
            out.push_str(&format!("- **project:** {p}\n"));
        }
        if let Some(n) = &t.notes {
            out.push_str(&format!("\n### Notes\n\n{n}\n"));
        }
        if !row.evidence.is_empty() {
            out.push_str("\n### Evidence\n\n");
            for ev in &row.evidence {
                out.push_str(&format!(
                    "- `{}` · {} · {}\n",
                    ev.kind,
                    ev.source_ref,
                    ev.observed_at.to_rfc3339()
                ));
                if let Some(s) = &ev.snippet {
                    out.push_str(&format!("  > {}\n", s.replace('\n', " ")));
                }
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::NewTask;
    use tempfile::tempdir;

    #[test]
    fn exports_json_file() {
        let dir = tempdir().unwrap();
        let paths = PulsePaths::new(dir.path());
        let store = Store::new(open_in_memory().unwrap());
        store
            .create_task(NewTask::manual("Export me please now"))
            .unwrap();
        let path = export_history(
            &store,
            &paths,
            ExportFormat::Json,
            None,
            None,
            Some(&dir.path().join("out.json")),
        )
        .unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("Export me please now"));
    }
}
