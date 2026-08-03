//! Parse model stdout into structured candidates / summaries.

use crate::types::{LlmError, Result, SummaryOut, TaskCandidateOut, TaskCopilotOut, TaskCopilotStep};

#[derive(serde::Deserialize)]
struct CandidatesWrapper {
    candidates: Vec<TaskCandidateOut>,
}

#[derive(serde::Deserialize)]
struct SummaryWrapper {
    text: String,
    #[serde(default)]
    highlights: Vec<String>,
}

#[derive(serde::Deserialize)]
struct TaskCopilotWrapper {
    answer: String,
    #[serde(default)]
    cited_task_ids: Vec<String>,
}

/// Extract first JSON object from text (strips fences if present).
pub fn extract_json_object(raw: &str) -> Result<String> {
    let s = raw.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```JSON"))
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s).trim();

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return Ok(v.to_string());
    }

    // Find first { ... } balanced-ish
    if let Some(start) = s.find('{') {
        let mut depth = 0i32;
        for (i, ch) in s[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let slice = &s[start..start + i + 1];
                        if serde_json::from_str::<serde_json::Value>(slice).is_ok() {
                            return Ok(slice.to_string());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    Err(LlmError::Parse(
        "no JSON object found in model output".into(),
    ))
}

pub fn parse_candidates(raw: &str) -> Result<Vec<TaskCandidateOut>> {
    let json = extract_json_object(raw)?;
    if let Ok(w) = serde_json::from_str::<CandidatesWrapper>(&json) {
        return Ok(normalize_candidates(w.candidates));
    }
    // bare array
    if let Ok(arr) = serde_json::from_str::<Vec<TaskCandidateOut>>(&json) {
        return Ok(normalize_candidates(arr));
    }
    Err(LlmError::Parse(format!(
        "expected candidates object, got: {}",
        json.chars().take(200).collect::<String>()
    )))
}

fn normalize_candidates(mut c: Vec<TaskCandidateOut>) -> Vec<TaskCandidateOut> {
    for item in &mut c {
        item.confidence = item.confidence.clamp(0.0, 1.0);
        item.title = item.title.trim().to_string();
    }
    c.retain(|c| c.title.chars().count() >= 12);
    c
}

pub fn parse_summary(raw: &str) -> Result<SummaryOut> {
    let json = extract_json_object(raw)?;
    if let Ok(w) = serde_json::from_str::<SummaryWrapper>(&json) {
        return Ok(SummaryOut {
            text: w.text,
            highlights: w.highlights,
        });
    }
    // fallback: whole text
    Ok(SummaryOut {
        text: raw.trim().to_string(),
        highlights: vec![],
    })
}

pub fn parse_task_copilot(raw: &str) -> Result<TaskCopilotOut> {
    let json = extract_json_object(raw)?;
    let output: TaskCopilotWrapper = serde_json::from_str(&json)
        .map_err(|e| LlmError::Parse(format!("invalid task copilot response: {e}")))?;
    let answer = output.answer.trim().to_string();
    if answer.is_empty() {
        return Err(LlmError::Parse(
            "task copilot response has no answer".into(),
        ));
    }
    Ok(TaskCopilotOut {
        answer,
        cited_task_ids: output.cited_task_ids,
    })
}

pub fn parse_task_copilot_step(raw: &str) -> Result<TaskCopilotStep> {
    let json = extract_json_object(raw)?;
    let step: TaskCopilotStep = serde_json::from_str(&json)
        .map_err(|e| LlmError::Parse(format!("invalid task copilot step: {e}")))?;
    match &step {
        TaskCopilotStep::ToolCall { tool, .. } if tool.trim().is_empty() => {
            Err(LlmError::Parse("task copilot tool call has no tool name".into()))
        }
        TaskCopilotStep::Final { answer, .. } if answer.trim().is_empty() => {
            Err(LlmError::Parse("task copilot final response has no answer".into()))
        }
        _ => Ok(step),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_candidates() {
        let raw = r#"```json
{"candidates":[{"title":"Implement export CSV button for reports","confidence":0.8,"notes":null,"suggested_next_action":null,"proposed_status":"Inbox","evidence_snippet":"please implement"}]}
```"#;
        let c = parse_candidates(raw).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c[0].title.contains("export"));
    }

    #[test]
    fn parses_grounded_task_copilot_response() {
        let answer = parse_task_copilot(
            r#"{"answer":"Focus on the migration task.","cited_task_ids":["task-123"]}"#,
        )
        .unwrap();
        assert_eq!(answer.answer, "Focus on the migration task.");
        assert_eq!(answer.cited_task_ids, vec!["task-123"]);
    }

    #[test]
    fn parses_task_copilot_tool_call() {
        let step = parse_task_copilot_step(
            r#"{"type":"tool_call","tool":"list_tasks","arguments":{"status":"Today","limit":5}}"#,
        )
        .unwrap();
        match step {
            TaskCopilotStep::ToolCall { tool, arguments } => {
                assert_eq!(tool, "list_tasks");
                assert_eq!(arguments["status"], "Today");
            }
            TaskCopilotStep::Final { .. } => panic!("expected a tool call"),
        }
    }
}
