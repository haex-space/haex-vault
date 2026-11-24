//! The check on writing (spec 097 FR-022, research R9): every title, description and text that is
//! stored, by an agent or by the user, is refused with one reason when it has the wrong form,
//! carries invisible characters, holds a secret, or carries the role markers of a model's prompt.
//!
//! Every rule reads the form of the characters, never words of a language: users talk to their
//! agents in any language, and a list of phrases could never cover them all. What a text means is
//! left to the model's instructions and to the user, who sees every write of an agent. The secrets
//! of the user's password manager are matched by [`holds_vault_secret`] against what the vault
//! holds. This check is not the barrier against foreign content: the question before a write after
//! foreign content (FR-019) and the framing of the memory as notes (FR-014) are.

use crate::extensions::bridge::header_validate::BIDI;
use crate::mcp_server::redact::redact_text;

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;

/// The part of a page a text is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Description,
    Body,
}

impl Field {
    /// The name used in error reasons (`too_long:title`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
            Self::Body => "body",
        }
    }
}

/// Why a text was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error("the text is empty")]
    Empty,
    #[error("the text must be one line")]
    LineBreak,
    #[error("the text carries invisible characters")]
    InvisibleCharacters,
    #[error("the text holds a password, key or token")]
    Secret,
    #[error("the text carries the role markers of a model's prompt")]
    RoleMarker,
}

impl CheckError {
    /// The code of the contract.
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::LineBreak => "line_break",
            Self::InvisibleCharacters => "invisible_characters",
            Self::Secret => "secret",
            Self::RoleMarker => "role_marker",
        }
    }
}

/// Checks `text` for `field`. Lengths are the limits' business (`super::limits`).
pub fn check(field: Field, text: &str) -> Result<(), CheckError> {
    if field != Field::Body {
        if text.trim().is_empty() {
            return Err(CheckError::Empty);
        }
        if text
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        {
            return Err(CheckError::LineBreak);
        }
    }
    if text.chars().any(invisible) {
        return Err(CheckError::InvisibleCharacters);
    }
    if holds_secret(text) {
        return Err(CheckError::Secret);
    }
    if role_marker(text) {
        return Err(CheckError::RoleMarker);
    }
    Ok(())
}

/// Zero-width characters besides the bidirectional marks.
const ZERO_WIDTH: [char; 5] = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'];

/// A character that changes what a reader sees without being seen: a control character other than
/// a line break or a tab, a zero-width character, a bidirectional mark, the Unicode tag block.
fn invisible(c: char) -> bool {
    (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || ZERO_WIDTH.contains(&c)
        || BIDI.contains(&c)
        || ('\u{E0000}'..='\u{E007F}').contains(&c)
}

/// Prefixes of tokens of well-known services, each followed by at least
/// [`TOKEN_REST_CHARS`] more characters.
const TOKEN_PREFIXES: [&str; 10] = [
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghs_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "nsec1",
];
const TOKEN_REST_CHARS: usize = 10;

/// A run of key characters this long, with a letter and a digit, reads as a key.
const KEY_RUN_CHARS: usize = 32;

fn holds_secret(text: &str) -> bool {
    if text.contains("-----BEGIN") && text.contains("PRIVATE KEY") {
        return true;
    }
    if authorization_credential(text) {
        return true;
    }
    // `redact_text` hides the word after "Bearer" and "Basic" whatever it is, which would refuse
    // "the basic idea"; a real token after them is caught by the prefixes and the key run.
    let without_schemes: Vec<&str> = text
        .split_whitespace()
        .filter(|word| !word.eq_ignore_ascii_case("bearer") && !word.eq_ignore_ascii_case("basic"))
        .collect();
    let plain = without_schemes.join(" ");
    if redact_text(&plain) != plain {
        return true;
    }
    text.split_whitespace().any(token_like) || key_run(text)
}

/// The general redactor recognizes `Basic` and `Bearer` as schemes, but the ordinary word "basic"
/// is removed below to avoid refusing phrases like "the basic idea". Keep credentials in an
/// authorization header detectable even when their value is too short to look like a token.
fn authorization_credential(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.windows(3).any(|header| {
        let name = header[0].trim_end_matches(':');
        (name.eq_ignore_ascii_case("authorization")
            || name.eq_ignore_ascii_case("proxy-authorization"))
            && header[0].ends_with(':')
            && (header[1].eq_ignore_ascii_case("basic") || header[1].eq_ignore_ascii_case("bearer"))
            && !header[2].is_empty()
    })
}

fn token_like(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
    if TOKEN_PREFIXES
        .iter()
        .any(|prefix| word.starts_with(prefix) && word.len() >= prefix.len() + TOKEN_REST_CHARS)
    {
        return true;
    }
    let parts: Vec<&str> = word.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts.iter().all(|part| {
            part.len() >= 8
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// A run of base64 or hex characters as long as a key. `-`, `/`, `.` and `:` end a run, so the
/// path of a web address is never one.
fn key_run(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '=' | '_')))
        .any(|run| {
            run.len() >= KEY_RUN_CHARS
                && run.chars().any(|c| c.is_ascii_digit())
                && run.chars().any(|c| c.is_ascii_alphabetic())
        })
}

/// The shortest value of the password manager that is matched; shorter ones ("2024") would match
/// ordinary text.
pub const VAULT_SECRET_MIN_CHARS: usize = 6;

/// Whether `text` holds one of `secrets`, the passwords and one-time-password secrets the user's
/// password manager stores (`store::vault_secrets`). Matched as written: a password is exact.
pub fn holds_vault_secret(text: &str, secrets: &[String]) -> bool {
    secrets
        .iter()
        .filter(|secret| secret.chars().count() >= VAULT_SECRET_MIN_CHARS)
        .any(|secret| text.contains(secret.as_str()))
}

/// Markers that open or close a role in the prompt formats of models, compared without case.
const ROLE_MARKERS: [&str; 8] = [
    "[inst]",
    "[/inst]",
    "<<sys>>",
    "<</sys>>",
    "<start_of_turn>",
    "<end_of_turn>",
    "<system>",
    "</system>",
];

/// A role marker of a model's prompt: one of [`ROLE_MARKERS`], or a special token of the form
/// `<|name|>` (`<|im_start|>`, `<|eot_id|>`). A pipe operator in code (`x |> f`) is none.
fn role_marker(text: &str) -> bool {
    let lower = text.to_lowercase();
    if ROLE_MARKERS.iter().any(|marker| lower.contains(marker)) {
        return true;
    }
    lower.match_indices("<|").any(|(at, _)| {
        let rest = &lower[at + 2..];
        rest.find("|>").is_some_and(|end| {
            end > 0
                && rest[..end]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    })
}
