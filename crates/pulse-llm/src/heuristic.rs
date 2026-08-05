//! Local heuristic task extraction (no network, no agent CLI).

use crate::types::{
    InferRequest, LlmClient, LlmError, Result, SummaryOut, SummaryRequest, TaskCandidateOut,
    TaskCopilotAgentRequest, TaskCopilotOut, TaskCopilotRequest, TaskCopilotStep, TaskCopilotTask,
    TaskCopilotToolResult,
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

    fn task_copilot_step(&self, req: &TaskCopilotAgentRequest) -> Result<TaskCopilotStep> {
        if req.transcript.is_empty() && req.remaining_tool_calls > 0 {
            let query = req.query.to_ascii_lowercase();
            let cloud_memory_available = req
                .tools
                .iter()
                .any(|tool| tool.name == "search_cloud_memory");
            let asks_for_prior_context = [
                "remember",
                "memory",
                "previous",
                "prior work",
                "last time",
                "what happened",
                "context",
                "decision",
                "checkpoint",
            ]
            .iter()
            .any(|phrase| query.contains(phrase));
            if cloud_memory_available && asks_for_prior_context {
                return Ok(TaskCopilotStep::ToolCall {
                    tool: "search_cloud_memory".into(),
                    arguments: serde_json::json!({ "query": req.query, "limit": 10 }),
                });
            }
            let status = if query.contains("blocked") || query.contains("waiting") {
                Some("Waiting")
            } else if query.contains("today")
                || query.contains("work on")
                || query.contains("focus")
            {
                Some("Today")
            } else {
                None
            };
            return Ok(TaskCopilotStep::ToolCall {
                tool: if status.is_some() {
                    "list_tasks".into()
                } else {
                    "search_tasks".into()
                },
                arguments: if let Some(status) = status {
                    serde_json::json!({ "status": status, "limit": 10 })
                } else {
                    serde_json::json!({ "query": req.query, "limit": 10 })
                },
            });
        }

        let tasks = req
            .transcript
            .iter()
            .rev()
            .find_map(tasks_from_tool_result)
            .unwrap_or_default();
        let answer = heuristic_task_answer(&TaskCopilotRequest {
            query: req.query.clone(),
            tasks,
        });
        let memory_summary = req
            .transcript
            .iter()
            .rev()
            .find(|result| result.tool == "search_cloud_memory")
            .and_then(|result| result.result.get("memories"))
            .and_then(serde_json::Value::as_array)
            .map(|memories| {
                memories
                    .iter()
                    .take(3)
                    .filter_map(|memory| memory.get("content").and_then(serde_json::Value::as_str))
                    .map(|content| format!("- {}", content.chars().take(500).collect::<String>()))
                    .collect::<Vec<_>>()
            })
            .filter(|memories| !memories.is_empty());
        let answer = if let Some(memories) = memory_summary {
            TaskCopilotOut {
                answer: format!(
                    "I found this related long-term activity memory in CockroachDB:\n{}\n\n{}",
                    memories.join("\n"),
                    answer.answer
                ),
                cited_task_ids: answer.cited_task_ids,
            }
        } else {
            answer
        };
        Ok(TaskCopilotStep::Final {
            answer: answer.answer,
            cited_task_ids: answer.cited_task_ids,
        })
    }
}

fn tasks_from_tool_result(result: &TaskCopilotToolResult) -> Option<Vec<TaskCopilotTask>> {
    if result.tool == "get_task" {
        return result
            .result
            .get("task")
            .cloned()
            .and_then(|task| serde_json::from_value(task).ok())
            .map(|task| vec![task]);
    }
    result
        .result
        .get("tasks")
        .cloned()
        .and_then(|tasks| serde_json::from_value(tasks).ok())
}

fn heuristic_task_answer(req: &TaskCopilotRequest) -> TaskCopilotOut {
    let query = req.query.trim().to_ascii_lowercase();
    let open: Vec<&TaskCopilotTask> = req
        .tasks
        .iter()
        .filter(|task| task.status != "Done")
        .collect();
    let waiting: Vec<&TaskCopilotTask> = req
        .tasks
        .iter()
        .filter(|task| task.status == "Waiting")
        .collect();
    let today: Vec<&TaskCopilotTask> = req
        .tasks
        .iter()
        .filter(|task| task.status == "Today")
        .collect();
    let in_progress: Vec<&TaskCopilotTask> = req
        .tasks
        .iter()
        .filter(|task| task.sync_outcome.as_deref() == Some("in_progress"))
        .collect();

    let selected = if query.contains("blocked") || query.contains("waiting") {
        waiting
    } else if query.contains("today")
        || query.contains("work on")
        || query.contains("focus")
        || query.contains("next")
    {
        if today.is_empty() {
            open
        } else {
            today
        }
    } else if query.contains("progress") || query.contains("active") {
        if in_progress.is_empty() {
            open
        } else {
            in_progress
        }
    } else {
        let terms: Vec<&str> = query
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|term| {
                term.len() >= 3
                    && !matches!(
                        *term,
                        "what" | "show" | "task" | "tasks" | "with" | "about" | "work"
                    )
            })
            .collect();
        let matching: Vec<&TaskCopilotTask> = req
            .tasks
            .iter()
            .filter(|task| {
                let haystack = format!(
                    "{} {} {}",
                    task.title,
                    task.notes.as_deref().unwrap_or(""),
                    task.suggested_next_action.as_deref().unwrap_or("")
                )
                .to_ascii_lowercase();
                !terms.is_empty() && terms.iter().any(|term| haystack.contains(term))
            })
            .collect();
        if matching.is_empty() {
            open
        } else {
            matching
        }
    };

    if selected.is_empty() {
        return TaskCopilotOut {
            answer: "I couldn't find an open task that answers that yet.".into(),
            cited_task_ids: vec![],
        };
    }

    let cited_task_ids: Vec<String> = selected
        .iter()
        .take(3)
        .map(|task| task.id.clone())
        .collect();
    let intro = if query.contains("blocked") || query.contains("waiting") {
        "Tasks waiting for attention:"
    } else if query.contains("today")
        || query.contains("work on")
        || query.contains("focus")
        || query.contains("next")
    {
        "Best current focus:"
    } else {
        "Here are the most relevant tasks:"
    };
    let lines = selected
        .iter()
        .take(3)
        .map(|task| {
            let next = task
                .suggested_next_action
                .as_deref()
                .map(|value| format!(" — next: {value}"))
                .unwrap_or_default();
            format!("- {} ({}){}", task.title, task.status, next)
        })
        .collect::<Vec<_>>()
        .join("\n");
    TaskCopilotOut {
        answer: format!("{intro}\n{lines}"),
        cited_task_ids,
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

    #[test]
    fn copilot_prioritizes_today_and_returns_task_citations() {
        let output = heuristic_task_answer(&TaskCopilotRequest {
            query: "What should I work on today?".into(),
            tasks: vec![
                TaskCopilotTask {
                    id: "today-task".into(),
                    title: "Implement the read-only task copilot".into(),
                    status: "Today".into(),
                    notes: None,
                    suggested_next_action: Some("Add constrained query tools".into()),
                    project: None,
                    sync_outcome: None,
                    updated_at: "2026-08-03T00:00:00Z".into(),
                },
                TaskCopilotTask {
                    id: "next-task".into(),
                    title: "Prepare the next product milestone".into(),
                    status: "Next".into(),
                    notes: None,
                    suggested_next_action: None,
                    project: None,
                    sync_outcome: None,
                    updated_at: "2026-08-03T00:00:00Z".into(),
                },
            ],
        });

        assert_eq!(output.cited_task_ids, vec!["today-task"]);
        assert!(output.answer.contains("Add constrained query tools"));
        assert!(!output.answer.contains("Prepare the next product milestone"));
    }

    #[test]
    fn copilot_agent_starts_with_a_read_only_tool_call() {
        let step = HeuristicClient::default()
            .task_copilot_step(&TaskCopilotAgentRequest {
                query: "What is blocked?".into(),
                tools: vec![],
                transcript: vec![],
                remaining_tool_calls: 2,
            })
            .unwrap();
        match step {
            TaskCopilotStep::ToolCall { tool, arguments } => {
                assert_eq!(tool, "list_tasks");
                assert_eq!(arguments["status"], "Waiting");
            }
            TaskCopilotStep::Final { .. } => panic!("expected a tool call"),
        }
    }
}
