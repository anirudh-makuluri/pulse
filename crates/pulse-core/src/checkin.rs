//! Apply check-in answers to task patches.

use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::models::{CheckInKind, TaskStatus, TaskUpdate};

/// Map check-in kind + answer into a task update.
pub fn apply_checkin_answer(kind: CheckInKind, answer: &Value) -> Result<TaskUpdate> {
    match kind {
        CheckInKind::IsDone => {
            let done = answer
                .get("done")
                .and_then(|v| v.as_bool())
                .or_else(|| match answer.as_str() {
                    Some("yes") | Some("y") | Some("true") => Some(true),
                    Some("no") | Some("n") | Some("false") => Some(false),
                    _ => None,
                });
            match done {
                Some(true) => Ok(TaskUpdate {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                }),
                Some(false) => {
                    let status = answer
                        .get("status")
                        .and_then(|s| s.as_str())
                        .and_then(TaskStatus::parse)
                        .unwrap_or(TaskStatus::Today);
                    Ok(TaskUpdate {
                        status: Some(status),
                        ..Default::default()
                    })
                }
                None => Err(PulseError::Validation(
                    "is_done answer needs {\"done\":true|false} or yes/no".into(),
                )),
            }
        }
        CheckInKind::StillActive => {
            let active = answer
                .get("active")
                .and_then(|v| v.as_bool())
                .or_else(|| match answer.as_str() {
                    Some("yes") | Some("y") | Some("true") => Some(true),
                    Some("no") | Some("n") | Some("false") => Some(false),
                    _ => None,
                });
            match active {
                Some(true) => {
                    let status = answer
                        .get("status")
                        .and_then(|s| s.as_str())
                        .and_then(TaskStatus::parse)
                        .unwrap_or(TaskStatus::Today);
                    Ok(TaskUpdate {
                        status: Some(status),
                        ..Default::default()
                    })
                }
                Some(false) => Ok(TaskUpdate {
                    status: Some(TaskStatus::Waiting),
                    ..Default::default()
                }),
                None => Err(PulseError::Validation(
                    "still_active answer needs {\"active\":true|false} or yes/no".into(),
                )),
            }
        }
        CheckInKind::NextStep => {
            let next = answer
                .get("next_action")
                .and_then(|v| v.as_str())
                .or_else(|| answer.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    PulseError::Validation(
                        "next_step answer needs {\"next_action\":\"...\"} or plain text".into(),
                    )
                })?;
            let status = answer
                .get("status")
                .and_then(|s| s.as_str())
                .and_then(TaskStatus::parse);
            Ok(TaskUpdate {
                suggested_next_action: Some(next),
                status,
                ..Default::default()
            })
        }
    }
}

/// Parse CLI answer: JSON object, or shorthand yes/no/text.
pub fn parse_answer_input(raw: &str) -> Result<Value> {
    let t = raw.trim();
    if t.starts_with('{') {
        serde_json::from_str(t).map_err(|e| PulseError::Validation(format!("bad answer json: {e}")))
    } else {
        Ok(Value::String(t.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn done_yes() {
        let u = apply_checkin_answer(CheckInKind::IsDone, &json!({"done": true})).unwrap();
        assert_eq!(u.status, Some(TaskStatus::Done));
    }

    #[test]
    fn still_active_no() {
        let u = apply_checkin_answer(CheckInKind::StillActive, &Value::String("no".into())).unwrap();
        assert_eq!(u.status, Some(TaskStatus::Waiting));
    }
}
