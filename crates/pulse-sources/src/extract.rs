//! Shared JSONL extraction: complete lines only, prefer user/assistant text.

use serde_json::Value;

/// Read `path` from `since_offset`, return (text from complete new lines, new_offset).
/// On file shrink / offset past EOF, reset to 0 and re-read.
pub fn read_complete_lines(
    path: &std::path::Path,
    since_offset: Option<u64>,
) -> std::io::Result<(String, u64, u64)> {
    use std::io::{Read, Seek, SeekFrom};

    let meta = std::fs::metadata(path)?;
    let size = meta.len();
    let mut offset = since_offset.unwrap_or(0);
    if offset > size {
        offset = 0;
    }

    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    // Only complete lines (ended by \n). Keep remainder unconsumed.
    let (complete, remainder_len) = match buf.rfind('\n') {
        Some(i) => (&buf[..=i], buf.len() - (i + 1)),
        None => ("", buf.len()),
    };
    let new_offset = size - remainder_len as u64;
    Ok((complete.to_string(), new_offset, size))
}

pub fn mtime_ms(path: &std::path::Path) -> std::io::Result<i64> {
    let meta = std::fs::metadata(path)?;
    let d = meta
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(d.as_millis() as i64)
}

/// Pull human-readable text from a JSONL value (version-tolerant).
pub fn text_from_json_value(v: &Value) -> Option<String> {
    // Prefer role-tagged messages.
    if let Some(msg) = v.get("message") {
        if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
            if matches!(role, "tool" | "tool_result") {
                return None;
            }
            if let Some(t) = content_to_text(msg.get("content")) {
                if !t.trim().is_empty() && !is_noise_text(&t) {
                    return Some(format!("[{role}] {t}"));
                }
            }
        } else if let Some(t) = content_to_text(msg.get("content")) {
            if !t.trim().is_empty() && !is_noise_text(&t) {
                return Some(t);
            }
        }
    }

    // Top-level type user/assistant (Claude)
    if let Some(ty) = v.get("type").and_then(|t| t.as_str()) {
        if matches!(ty, "user" | "assistant" | "human") {
            if let Some(msg) = v.get("message") {
                if let Some(t) = content_to_text(msg.get("content")) {
                    if !t.trim().is_empty() && !is_noise_text(&t) {
                        return Some(format!("[{ty}] {t}"));
                    }
                }
            }
            if let Some(t) = v.get("content").and_then(|c| c.as_str()) {
                if !is_noise_text(t) {
                    return Some(format!("[{ty}] {t}"));
                }
            }
        }
    }

    // Codex: payload with role
    if let Some(payload) = v.get("payload") {
        if let Some(role) = payload.get("role").and_then(|r| r.as_str()) {
            if matches!(role, "user" | "assistant" | "human") {
                if let Some(t) = content_to_text(payload.get("content")) {
                    if !t.trim().is_empty() && !is_noise_text(&t) {
                        return Some(format!("[{role}] {t}"));
                    }
                }
            }
        }
        // event_msg with message text
        if let Some(t) = payload.get("message").and_then(|m| m.as_str()) {
            if !is_noise_text(t) {
                return Some(t.to_string());
            }
        }
        if let Some(t) = payload.get("text").and_then(|m| m.as_str()) {
            if !is_noise_text(t) {
                return Some(t.to_string());
            }
        }
    }

    // Generic fields
    for key in ["text", "content", "message"] {
        if let Some(t) = v.get(key).and_then(|x| x.as_str()) {
            if !is_noise_text(t) {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn content_to_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(ty) = item.get("type").and_then(|t| t.as_str()) {
                if matches!(ty, "tool_use" | "tool_result" | "function_call" | "function_call_output")
                {
                    continue;
                }
                if matches!(ty, "thinking") {
                    continue;
                }
            }
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(t.to_string());
            } else if let Some(t) = item.get("content").and_then(|t| t.as_str()) {
                // skip tool_result dumps somewhat
                if t.len() < 500 {
                    parts.push(t.to_string());
                }
            }
        }
        if parts.is_empty() {
            return None;
        }
        return Some(parts.join("\n"));
    }
    None
}

fn is_noise_text(t: &str) -> bool {
    let t = t.trim();
    if t.is_empty() {
        return true;
    }
    t.starts_with("<local-command")
        || t.starts_with("<command-name>")
        || t.starts_with("<command-message>")
        || t.starts_with("<permissions instructions>")
        || t.starts_with("<environment_context>")
}

/// Extract candidate text from a chunk of complete JSONL lines.
pub fn extract_text_from_jsonl_chunk(chunk: &str, max_bytes: usize) -> String {
    let mut pieces = Vec::new();
    for line in chunk.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                if let Some(t) = text_from_json_value(&v) {
                    pieces.push(t);
                }
            }
            Err(_) => {
                // Non-JSON line — include truncated
                let t: String = line.chars().take(200).collect();
                if !is_noise_text(&t) {
                    pieces.push(t);
                }
            }
        }
    }
    let joined = pieces.join("\n");
    if joined.len() <= max_bytes {
        joined
    } else {
        // Tail-biased window
        let start = joined.len().saturating_sub(max_bytes);
        // snap to next char boundary
        let mut s = start;
        while s < joined.len() && !joined.is_char_boundary(s) {
            s += 1;
        }
        joined[s..].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn complete_lines_only() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "line1\nline2\npartial").unwrap();
        let (text, off, size) = read_complete_lines(f.path(), Some(0)).unwrap();
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
        assert!(!text.contains("partial"));
        assert_eq!(off, size - "partial".len() as u64);
    }

    #[test]
    fn claude_user_message_extract() {
        let line = r#"{"type":"user","message":{"role":"user","content":"Please implement dark mode toggle"}}"#;
        let t = extract_text_from_jsonl_chunk(line, 10_000);
        assert!(t.to_lowercase().contains("dark mode"));
    }
}
