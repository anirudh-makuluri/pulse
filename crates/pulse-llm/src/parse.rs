//! Parse model stdout into structured candidates / summaries.

use crate::types::{LlmError, Result, SummaryOut, TaskCandidateOut};

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
}
