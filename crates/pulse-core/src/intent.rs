//! Deterministic, local parsing for the Pulse omnibox.

use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmniboxIntent {
    CreateTask, CompleteTask, DeleteTask, SearchActivity, CreateReminder,
    SnoozeReminder, ResumeTask, TransferTask, OpenContext, Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedOmniboxIntent {
    pub intent: OmniboxIntent,
    pub raw: String,
    pub subject: String,
    pub due_at: Option<DateTime<Utc>>,
}

/// Parse an intentionally small command language. Model output is never used to
/// execute actions; unsupported text becomes a new task draft.
pub fn parse_omnibox(input: &str, now: DateTime<Local>) -> ParsedOmniboxIntent {
    let raw = input.trim().to_string();
    let normalized = raw.to_ascii_lowercase();
    let (intent, prefix_len) = if starts(&normalized, &["remind me", "remind", "reminder"]) {
        (OmniboxIntent::CreateReminder, reminder_prefix_len(&normalized))
    } else if starts(&normalized, &["snooze", "later"]) {
        (OmniboxIntent::SnoozeReminder, first_word_len(&normalized))
    } else if starts(&normalized, &["done", "complete", "finish"]) {
        (OmniboxIntent::CompleteTask, first_word_len(&normalized))
    } else if starts(&normalized, &["delete", "remove", "cancel task"]) {
        (OmniboxIntent::DeleteTask, first_word_len(&normalized))
    } else if starts(&normalized, &["find", "search", "show"]) {
        (OmniboxIntent::SearchActivity, first_word_len(&normalized))
    } else if starts(&normalized, &["resume", "continue"]) {
        (OmniboxIntent::ResumeTask, first_word_len(&normalized))
    } else if starts(&normalized, &["handoff", "transfer", "send to codex"]) {
        (OmniboxIntent::TransferTask, first_word_len(&normalized))
    } else if starts(&normalized, &["open context", "open this", "context"]) {
        (OmniboxIntent::OpenContext, first_word_len(&normalized))
    } else if starts(&normalized, &["add", "create", "task", "todo"]) {
        (OmniboxIntent::CreateTask, first_word_len(&normalized))
    } else { (OmniboxIntent::CreateTask, 0) };
    let rest = raw.get(prefix_len..).unwrap_or("").trim();
    let (subject, due_at) = if intent == OmniboxIntent::CreateReminder {
        split_reminder_subject(rest, now)
    } else if intent == OmniboxIntent::SnoozeReminder {
        (rest.to_string(), parse_relative_time(rest, now))
    } else { (rest.to_string(), None) };
    ParsedOmniboxIntent { intent, raw, subject, due_at }
}

fn starts(input: &str, words: &[&str]) -> bool {
    words.iter().any(|word| input == *word || input.starts_with(&format!("{word} ")))
}
fn first_word_len(input: &str) -> usize { input.find(char::is_whitespace).unwrap_or(input.len()) }
fn reminder_prefix_len(input: &str) -> usize {
    ["remind me", "reminder", "remind"].iter().find_map(|p| input.strip_prefix(p).map(|_| p.len())).unwrap_or(0)
}
fn split_reminder_subject(value: &str, now: DateTime<Local>) -> (String, Option<DateTime<Utc>>) {
    for marker in [" in ", " tomorrow", " at "] {
        if let Some(index) = value.to_ascii_lowercase().rfind(marker) {
            if let Some(due) = parse_relative_time(value[index..].trim(), now) {
                return (
                    value[..index]
                        .trim()
                        .trim_start_matches("me to ")
                        .trim_start_matches("to ")
                        .to_string(),
                    Some(due),
                );
            }
        }
    }
    (
        value
            .trim_start_matches("me to ")
            .trim_start_matches("to ")
            .to_string(),
        None,
    )
}

/// Validate only schedule strings Pulse can execute deterministically.
pub fn parse_relative_time(value: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(caps) = regex::Regex::new(r"^(?:in )?(\d+)\s*(minute|minutes|hour|hours|day|days)$").ok()?.captures(&value) {
        let amount: i64 = caps.get(1)?.as_str().parse().ok()?;
        let duration = match caps.get(2)?.as_str() { "minute"|"minutes" => Duration::minutes(amount), "hour"|"hours" => Duration::hours(amount), "day"|"days" => Duration::days(amount), _ => return None };
        return Some((now + duration).with_timezone(&Utc));
    }
    let tomorrow = value.strip_prefix("tomorrow")?;
    let time = match tomorrow.trim() {
        "" => NaiveTime::from_hms_opt(9, 0, 0)?, "morning" => NaiveTime::from_hms_opt(9, 0, 0)?,
        "afternoon" => NaiveTime::from_hms_opt(13, 0, 0)?, "evening" => NaiveTime::from_hms_opt(18, 0, 0)?,
        other => parse_clock(other.trim_start_matches("at "))?,
    };
    Local.from_local_datetime(&now.date_naive().succ_opt()?.and_time(time)).single().map(|d| d.with_timezone(&Utc))
}
fn parse_clock(value: &str) -> Option<NaiveTime> {
    for format in ["%H:%M", "%I:%M %p", "%I %p"] { if let Ok(t) = NaiveTime::parse_from_str(&value.to_ascii_uppercase(), format) { return Some(t); } }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reminder() {
        let p = parse_omnibox("Remind me to review the PR in 30 minutes", Local::now());
        assert_eq!(p.intent, OmniboxIntent::CreateReminder);
        assert_eq!(p.subject, "review the PR");
        assert!(p.due_at.is_some());
    }

    #[test]
    fn routes_crud_search_and_continuity_without_an_llm() {
        let now = Local::now();
        assert_eq!(parse_omnibox("add upgrade dependencies", now).intent, OmniboxIntent::CreateTask);
        assert_eq!(parse_omnibox("done upgrade dependencies", now).intent, OmniboxIntent::CompleteTask);
        assert_eq!(parse_omnibox("delete old migration", now).intent, OmniboxIntent::DeleteTask);
        assert_eq!(parse_omnibox("find authentication", now).intent, OmniboxIntent::SearchActivity);
        assert_eq!(parse_omnibox("resume authentication", now).intent, OmniboxIntent::ResumeTask);
        assert_eq!(parse_omnibox("send to codex authentication", now).intent, OmniboxIntent::TransferTask);
        assert_eq!(parse_omnibox("open context authentication", now).intent, OmniboxIntent::OpenContext);
    }
}
