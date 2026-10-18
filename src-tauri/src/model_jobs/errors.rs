use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskErrorCode {
    ConsentRequired,
    ConsentDenied,
    UnsupportedCapability,
    ProfileUnavailable,
    InvalidInput,
    InvalidResult,
    Timeout,
    Cancelled,
    InferenceFailed,
    LimitExceeded,
    /// No model the extension may use fits what a step needs (spec 055).
    #[allow(
        dead_code,
        reason = "used by the job runner of the next step of delivery B"
    )]
    NoCapableModel,
    /// Every model tried gave output that was dropped: empty, a loop, a script switch, not JSON or
    /// against the format.
    #[allow(
        dead_code,
        reason = "used by the job runner of the next step of delivery B"
    )]
    Unusable,
    /// No model could be reached at all.
    #[allow(
        dead_code,
        reason = "used by the job runner of the next step of delivery B"
    )]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskError {
    pub code: TaskErrorCode,
    pub message: String,
}

impl TaskError {
    pub fn new(code: TaskErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Longest message handed to an extension.
const MAX_MESSAGE_CHARS: usize = 200;

/// Makes a message safe to hand to an extension (spec 055 FR-021): no address, no key and no
/// piece of an answer body. Provider and transport errors carry the URL they failed on (often an
/// address in the own network) and sometimes the key; both are replaced, the rest is cut short.
/// `known` are the values the caller knows to be secret (the provider's key and address): they go
/// first, whatever their shape; the shapes below catch the rest.
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
pub fn sanitize(message: &str, known: &[&str]) -> String {
    let mut message = message.to_string();
    for secret in known.iter().filter(|secret| secret.trim().len() >= 4) {
        message = message.replace(secret, "[redacted]");
    }
    let cleaned: Vec<String> = message
        .split_whitespace()
        .map(|token| {
            let bare = token.trim_matches(|c: char| "()[]{}<>\"',;:.".contains(c));
            if looks_like_address(bare) {
                return token.replace(bare, "[address]");
            }
            // `api_key=sk-…` or `"key":"sk-…"`: the value is a part of the token.
            let mut token = token.to_string();
            for part in bare.split(|c: char| "=:\"'".contains(c)) {
                if looks_like_secret(part) {
                    token = token.replace(part, "[redacted]");
                }
            }
            token
        })
        .collect();
    let text = cleaned.join(" ");
    if text.chars().count() <= MAX_MESSAGE_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_MESSAGE_CHARS).collect();
    format!("{cut}…")
}

/// Host names that only exist in the own network.
const PRIVATE_SUFFIXES: [&str; 5] = [".local", ".lan", ".internal", ".home.arpa", ".localhost"];

/// A URL, an IP address, a host with a port or a host name of the own network.
fn looks_like_address(token: &str) -> bool {
    if token.contains("://") {
        return true;
    }
    let lower = token.to_ascii_lowercase();
    if lower == "localhost"
        || PRIVATE_SUFFIXES
            .iter()
            .any(|suffix| lower.ends_with(suffix))
    {
        return true;
    }
    // IPv6 is written with `::` or in full with seven colons; a time like `00:05:00` is neither.
    let ipv6_shape = token.contains("::") || token.matches(':').count() == 7;
    if ipv6_shape
        && token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b':' || byte == b'.' || byte == b'%')
    {
        return true;
    }
    let host = match token.rsplit_once(':') {
        Some((host, port))
            if (2..=5).contains(&port.len())
                && port.bytes().all(|b| b.is_ascii_digit())
                && host.bytes().any(|b| b.is_ascii_alphabetic() || b == b'.') =>
        {
            host
        }
        _ => token,
    };
    let is_ipv4 = host.split('.').count() == 4
        && host
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    is_ipv4 || (host != token && !host.is_empty())
}

/// A long opaque string: a key or a token. Model names (`qwen2.5-vl-7b-instruct`) and paths are
/// lower case and stay; a key mixes cases, is very long or carries a key prefix.
fn looks_like_secret(token: &str) -> bool {
    let shape = token.len() >= 20
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/' | b'+'))
        && token.bytes().any(|b| b.is_ascii_digit())
        && token.bytes().any(|b| b.is_ascii_alphabetic());
    let mixed_case = token.bytes().any(|b| b.is_ascii_uppercase())
        && token.bytes().any(|b| b.is_ascii_lowercase());
    let key_prefix = ["sk-", "sk_", "hf_", "AIza", "ghp_", "gsk_", "xai-"]
        .iter()
        .any(|prefix| token.starts_with(prefix));
    shape && (mixed_case || key_prefix || token.len() >= 32)
}
