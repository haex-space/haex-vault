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
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
pub fn sanitize(message: &str) -> String {
    let cleaned: Vec<String> = message
        .split_whitespace()
        .map(|token| {
            let bare = token.trim_matches(|c: char| "()[]{}<>\"',;:.".contains(c));
            if looks_like_address(bare) {
                token.replace(bare, "[address]")
            } else if looks_like_secret(bare) {
                token.replace(bare, "[redacted]")
            } else {
                token.to_string()
            }
        })
        .collect();
    let text = cleaned.join(" ");
    if text.chars().count() <= MAX_MESSAGE_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_MESSAGE_CHARS).collect();
    format!("{cut}…")
}

/// A URL, an IPv4 address or a host with a port.
fn looks_like_address(token: &str) -> bool {
    if token.contains("://") {
        return true;
    }
    if token.matches(':').count() >= 2
        && token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b':' || byte == b'.' || byte == b'%')
    {
        return true;
    }
    let host = match token.rsplit_once(':') {
        Some((host, port))
            if (2..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit()) =>
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

/// A long opaque string made of letters, digits and a few signs: a key or a token.
fn looks_like_secret(token: &str) -> bool {
    token.len() >= 20
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && token.bytes().any(|b| b.is_ascii_digit())
        && token.bytes().any(|b| b.is_ascii_alphabetic())
}
