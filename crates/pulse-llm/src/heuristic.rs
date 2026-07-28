//! Local heuristic task extraction (no network, no agent CLI).

use crate::types::{
    InferRequest, LlmClient, LlmError, Result, SummaryOut, SummaryRequest, TaskCandidateOut,
};

pub struct HeuristicClient {
    pub max_confidence: f64,
}

impl Default for HeuristicClient {
    fn default() -> Self {
        Self {
            max_confidence: 0.45,
        }
    }
}

impl LlmClient for HeuristicClient {
    fn backend_id(&self) -> &str {
        "heuristic"
    }

    fn infer_tasks(&self, req: &InferRequest) -> Result<Vec<TaskCandidateOut>> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in req.candidate_text.lines() {
            if out.len() >= req.max_candidates {
                break;
            }
            if let Some(title) = line_to_title(line) {
                let key = title.to_ascii_lowercase();
                if !seen.insert(key) {
                    continue;
                }
                let snippet: String = line.chars().take(200).collect();
                out.push(TaskCandidateOut {
                    title,
                    notes: Some(format!("Inferred from {} session", req.source)),
                    confidence: self.max_confidence.min(0.45),
                    suggested_next_action: None,
                    proposed_status: Some("Inbox".into()),
                    evidence_snippet: Some(snippet),
                    match_task_id: None,
                    source_session_id: None,
                    sync_outcome: Some("unclear".into()),
                    sync_outcome_confidence: Some(0.0),
                });
            }
        }

        // If nothing matched but we have substantial user text, take first user line.
        if out.is_empty() {
            if let Some(title) = first_userish_title(&req.candidate_text) {
                out.push(TaskCandidateOut {
                    title,
                    notes: Some(format!("Inferred from {} session", req.source)),
                    confidence: 0.35,
                    suggested_next_action: None,
                    proposed_status: Some("Inbox".into()),
                    evidence_snippet: Some(req.candidate_text.chars().take(200).collect()),
                    match_task_id: None,
                    source_session_id: None,
                    sync_outcome: Some("unclear".into()),
                    sync_outcome_confidence: Some(0.0),
                });
            }
        }

        Ok(out)
    }

    fn summarize_day(&self, req: &SummaryRequest) -> Result<SummaryOut> {
        let mut highlights = Vec::new();
        for line in req.task_lines.iter().take(8) {
            highlights.push(line.clone());
        }
        let text = if req.task_lines.is_empty() {
            format!("# {} summary\n\nNo tasks recorded for this day.", req.day)
        } else {
            format!(
                "# {} summary\n\n{} task line(s).\n\n{}",
                req.day,
                req.task_lines.len(),
                req.task_lines
                    .iter()
                    .map(|l| format!("- {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        Ok(SummaryOut { text, highlights })
    }
}

fn line_to_title(line: &str) -> Option<String> {
    let raw = strip_role_prefix(line).trim();
    if raw.chars().count() < 12 {
        return None;
    }
    // skip pure code-ish
    if looks_like_code(raw) {
        return None;
    }

    let lower = raw.to_ascii_lowercase();

    // Checklist
    if let Some(rest) = raw
        .strip_prefix("- [ ]")
        .or_else(|| raw.strip_prefix("* [ ]"))
    {
        return clean_title(rest);
    }
    if lower.contains("todo:") || lower.contains("todo ") {
        if let Some(idx) = lower.find("todo") {
            let rest = raw[idx..].trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ':');
            return clean_title(rest);
        }
    }
    if lower.contains("fixme") {
        return clean_title(raw);
    }

    // Imperative / request verbs
    const VERBS: &[&str] = &[
        "implement",
        "add ",
        "fix ",
        "create ",
        "build ",
        "update ",
        "write ",
        "please ",
        "we need",
        "we want",
        "can you",
        "let's ",
        "make ",
        "refactor ",
        "ship ",
    ];
    if VERBS.iter().any(|v| lower.contains(v)) {
        return clean_title(raw);
    }

    None
}

fn first_userish_title(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = strip_role_prefix(line);
        if t.chars().count() >= 12 && !looks_like_code(t) {
            return clean_title(t);
        }
    }
    None
}

fn strip_role_prefix(line: &str) -> &str {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("[user] ") {
        return rest;
    }
    if let Some(rest) = line.strip_prefix("[human] ") {
        return rest;
    }
    if let Some(rest) = line.strip_prefix("[assistant] ") {
        return rest;
    }
    line
}

fn looks_like_code(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("import ")
        || t.starts_with("use ")
        || t.starts_with("fn ")
        || t.starts_with("pub ")
        || t.starts_with("const ")
        || t.starts_with("```")
        || t.contains("::{")
        || (t.contains('{') && t.contains('}') && t.len() < 40)
}

fn clean_title(s: &str) -> Option<String> {
    let mut t: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    // strip surrounding quotes
    t = t.trim_matches(|c| c == '"' || c == '\'').to_string();
    if t.chars().count() < 12 {
        return None;
    }
    // Cap title length
    if t.chars().count() > 120 {
        t = t.chars().take(117).collect::<String>() + "...";
    }
    Some(t)
}

// keep LlmError used
#[allow(dead_code)]
fn _e() -> LlmError {
    LlmError::Msg("x".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_todo_and_please() {
        let h = HeuristicClient::default();
        let req = InferRequest {
            source: "claude".into(),
            source_ref: "x".into(),
            session_id: "s".into(),
            project: Some("app".into()),
            candidate_text: "[user] Please implement the export CSV button for reports\nnoise"
                .into(),
            max_candidates: 5,
        };
        let out = h.infer_tasks(&req).unwrap();
        assert!(!out.is_empty());
        assert!(out[0].title.to_lowercase().contains("export"));
        assert!(out[0].confidence <= 0.45);
    }
}
