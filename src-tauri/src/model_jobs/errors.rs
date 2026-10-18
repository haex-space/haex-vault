//! The errors of a model job (spec 055, `contracts/bridge.md` §Fehlercodes): what the start answers
//! with, what the completion event carries, and the sanitising of a message before an extension
//! sees it (FR-021).

use serde::Serialize;
use serde_json::json;

use super::job::JobError;
use super::state::TooManyJobs;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};

/// Why a job did not give a result, as the extension reads it: in the completion event and, for
/// the first two, in the `details.reason` of an answer to the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobErrorCode {
    InvalidInput,
    LimitExceeded,
    /// No model the extension may use fits what a step needs (also when a grant was withdrawn
    /// while the job ran).
    NoCapableModel,
    /// The user refused the grant or the consent.
    ConsentDenied,
    /// Every model tried gave output that was dropped: empty, a loop, a script switch, not JSON or
    /// against the format.
    Unusable,
    /// No model could be reached at all.
    Unavailable,
    Timeout,
    Cancelled,
}

impl JobErrorCode {
    /// The spelling in `details.reason` and in the event, the one `serde` writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::LimitExceeded => "limit_exceeded",
            Self::NoCapableModel => "no_capable_model",
            Self::ConsentDenied => "consent_denied",
            Self::Unusable => "unusable",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }

    /// The code of a bridge answer: the fixed numbers of the vault-sdk. Only the first four can
    /// answer a start; the others end a running job and reach the extension in the event, but
    /// the mapping is total.
    fn bridge_code(self) -> ExtensionErrorCode {
        match self {
            Self::InvalidInput => ExtensionErrorCode::Validation,
            Self::LimitExceeded => ExtensionErrorCode::LimitExceeded,
            Self::ConsentDenied => ExtensionErrorCode::PermissionDenied,
            Self::NoCapableModel
            | Self::Unusable
            | Self::Unavailable
            | Self::Timeout
            | Self::Cancelled => ExtensionErrorCode::NotAvailable,
        }
    }
}

/// The `error` of a completion event: a code, a message safe for the extension, and the step it
/// ended in when there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailure {
    pub code: JobErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
}

impl JobFailure {
    /// `known` are the values the caller knows to be secret (the provider's key and address); the
    /// message is sanitised here, so no failure leaves with an address, a key or a piece of an
    /// answer body.
    pub fn new(code: JobErrorCode, message: &str, known: &[&str]) -> Self {
        Self {
            code,
            message: sanitize(message, known),
            step_id: None,
        }
    }

    pub fn at_step(mut self, step_id: &str) -> Self {
        self.step_id = Some(step_id.to_owned());
        self
    }

    /// The same failure as the answer of a call.
    pub fn to_bridge(&self) -> BridgeError {
        bridge_error(self.code, &self.message, self.code.as_str())
    }
}

/// An answer of a call: the fixed number of `code`, the message, and `details.reason`.
pub fn bridge_error(code: JobErrorCode, message: &str, reason: &str) -> BridgeError {
    BridgeError::new(code.bridge_code(), message).with_details(json!({ "reason": reason }))
}

/// The answer to a start whose job was refused before any model was asked: a form that is wrong
/// is `invalid_input`, a limit is named in `reason` and in `limit`.
pub fn refusal(error: &JobError) -> BridgeError {
    match error {
        JobError::Invalid(message) => {
            bridge_error(JobErrorCode::InvalidInput, message, "invalid_input")
        }
        JobError::Limit(name) => limit_exceeded(name),
    }
}

/// The frame runs too many jobs.
pub fn too_many(_: TooManyJobs) -> BridgeError {
    limit_exceeded("active_jobs")
}

fn limit_exceeded(name: &str) -> BridgeError {
    BridgeError::new(
        ExtensionErrorCode::LimitExceeded,
        format!("the limit {name} is exceeded"),
    )
    .with_details(json!({ "reason": name, "limit": name }))
}

/// Longest message handed to an extension.
const MAX_MESSAGE_CHARS: usize = 200;

/// Makes a message safe to hand to an extension (spec 055 FR-021): no address, no key and no
/// piece of an answer body. Provider and transport errors carry the URL they failed on (often an
/// address in the own network) and sometimes the key; both are replaced, the rest is cut short.
/// `known` are the values the caller knows to be secret (the provider's key and address): they go
/// first, whatever their shape; the shapes below catch the rest.
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
