//! Dedup key helpers for inferred tasks.

use sha2::{Digest, Sha256};

use crate::models::TaskSource;

/// Normalize a title for fingerprinting: lowercase, collapse whitespace, strip light punctuation.
pub fn normalize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_space = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_space = false;
        } else if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        }
        // drop other punctuation
    }
    out.trim().to_string()
}

/// `sha256_hex(source || ":" || session || ":" || fingerprint)` truncated fingerprint to 80 chars.
pub fn compute_dedup_key(source: TaskSource, session_id: &str, title: &str) -> String {
    let fp = normalize_title(title);
    let fp: String = fp.chars().take(80).collect();
    let material = format!("{}:{}:{}", source.as_str(), session_id, fp);
    let digest = Sha256::digest(material.as_bytes());
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses() {
        assert_eq!(normalize_title("  Hello, World!! "), "hello world");
    }

    #[test]
    fn dedup_stable() {
        let a = compute_dedup_key(TaskSource::Claude, "sess1", "Fix the bug");
        let b = compute_dedup_key(TaskSource::Claude, "sess1", "fix the bug!!");
        assert_eq!(a, b);
        let c = compute_dedup_key(TaskSource::Codex, "sess1", "Fix the bug");
        assert_ne!(a, c);
    }
}
