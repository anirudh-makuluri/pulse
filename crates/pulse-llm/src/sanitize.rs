//! Best-effort secret redaction before remote LLM / for stored evidence.

use regex::Regex;
use std::sync::OnceLock;

pub struct RedactedText {
    pub text: String,
}

pub fn redact_for_remote(input: &str) -> RedactedText {
    let mut text = input.to_string();

    // PEM blocks
    static PEM: OnceLock<Regex> = OnceLock::new();
    let pem = PEM.get_or_init(|| {
        Regex::new(r"(?s)-----BEGIN [^-]+-----.*?-----END [^-]+-----").unwrap()
    });
    text = pem.replace_all(&text, "[REDACTED_PEM]").into_owned();

    // Assignment secrets
    static ASSIGN: OnceLock<Regex> = OnceLock::new();
    let assign = ASSIGN.get_or_init(|| {
        Regex::new(
            r#"(?i)(api[_-]?key|secret|password|token|authorization)\s*[:=]\s*["']?[^\s"',;]+"#,
        )
        .unwrap()
    });
    text = assign
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}=[REDACTED]", &caps[1])
        })
        .into_owned();

    // Bearer
    static BEARER: OnceLock<Regex> = OnceLock::new();
    let bearer = BEARER.get_or_init(|| Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-\._~\+\/]+=*").unwrap());
    text = bearer.replace_all(&text, "Bearer [REDACTED]").into_owned();

    // Common key prefixes
    static KEYS: OnceLock<Regex> = OnceLock::new();
    let keys = KEYS.get_or_init(|| {
        Regex::new(r"\b(sk-[A-Za-z0-9]{10,}|ghp_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{12,}|xai-[A-Za-z0-9]{10,})\b")
            .unwrap()
    });
    text = keys.replace_all(&text, "[REDACTED_KEY]").into_owned();

    // Drop .env-ish lines
    let mut cleaned = String::with_capacity(text.len());
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains(".env")
            || lower.contains("credentials.json")
            || lower.contains("id_rsa")
        {
            cleaned.push_str("[REDACTED_LINE]\n");
        } else {
            cleaned.push_str(line);
            cleaned.push('\n');
        }
    }

    RedactedText { text: cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sk_and_assignment() {
        let r = redact_for_remote("API_KEY=sk-abcdefghijklmnop password=hunter2");
        assert!(!r.text.contains("sk-abcdefghijklmnop"));
        assert!(!r.text.contains("hunter2"));
        assert!(r.text.contains("REDACTED"));
    }
}
