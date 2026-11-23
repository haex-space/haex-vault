//! The check on writing (spec 097 FR-022, research R9): every title, description and text that is
//! stored, by an agent or by the user, is refused with one reason when it has the wrong form,
//! carries invisible characters, holds a secret, or reads as an instruction to an agent.
//!
//! This is not the barrier against foreign content: a preference is often phrased like an
//! instruction, so the lists only catch text aimed at the agent itself. The question before a write
//! after foreign content (FR-019) and the framing of the memory as notes (FR-014) are the barrier.

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
    #[error("the text reads as an instruction to an agent")]
    Instruction,
}

impl CheckError {
    /// The code of the contract.
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::LineBreak => "line_break",
            Self::InvisibleCharacters => "invisible_characters",
            Self::Secret => "secret",
            Self::Instruction => "instruction",
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
    if addresses_an_agent(text) {
        return Err(CheckError::Instruction);
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

/// Words that name a password, German and English, compared lowercase; also as the end of a
/// compound ("WLAN-Passwort", "Bankpasswort").
const PASSWORD_WORDS: [&str; 4] = ["passwort", "kennwort", "password", "passcode"];

/// Short words that name a PIN; only whole or after a hyphen ("SIM-PIN"), so "Rubin" is none.
const PIN_WORDS: [&str; 3] = ["pin", "pincode", "pin-code"];

fn names_a_password(word: &str) -> bool {
    PASSWORD_WORDS.iter().any(|name| word.ends_with(name))
        || PIN_WORDS
            .iter()
            .any(|name| word == *name || word.ends_with(&format!("-{name}")))
}

/// Words between a password word and its value ("mein Passwort ist …").
const CONNECTORS: [&str; 6] = ["ist", "is", "lautet", "=", ":", "is:"];

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
    let words: Vec<&str> = text.split_whitespace().collect();
    words.iter().any(|word| token_like(word)) || key_run(text) || password_value(&words)
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

/// A password word followed by its value: after `:` or `=` any word, after "ist"/"is"/"lautet"
/// or directly a word with a digit ("PIN 4711"), so "PIN vergessen" passes.
fn password_value(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(at, word)| {
        let lower = word.to_lowercase();
        let (name, inline_value, assigned) = match lower.find([':', '=']) {
            Some(separator) => (&lower[..separator], Some(&lower[separator + 1..]), true),
            None => (lower.as_str(), None, false),
        };
        if !names_a_password(name) {
            return false;
        }
        let mut rest = words[at + 1..].iter();
        let mut next = inline_value
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| rest.next().map(|w| w.to_lowercase()));
        let mut assigned = assigned;
        while let Some(word) = next.as_deref() {
            if !CONNECTORS.contains(&word) {
                break;
            }
            assigned |= matches!(word, "=" | ":" | "is:" | "ist" | "is" | "lautet");
            next = rest.next().map(|w| w.to_lowercase());
        }
        next.is_some_and(|value| assigned || value.chars().any(|c| c.is_ascii_digit()))
    })
}

/// Phrases that speak to an agent about its rules or its role, lowercase.
const INSTRUCTION_PHRASES: [&str; 20] = [
    "system prompt",
    "systemprompt",
    "system-prompt",
    "you are now",
    "du bist jetzt",
    "from now on you are",
    "ab jetzt bist du",
    "<|",
    "|>",
    "[inst]",
    "[/inst]",
    "### system",
    "### instruction",
    "<system>",
    "</system>",
    "developer message",
    "jailbreak",
    "ignore previous",
    "ignore all previous",
    "ignore the above",
];

/// Requests to keep something from the user, lowercase.
const CONCEALMENT_PHRASES: [&str; 12] = [
    "don't tell the user",
    "do not tell the user",
    "never tell the user",
    "without telling the user",
    "hide this from the user",
    "keep this from the user",
    "don't mention this",
    "do not mention this",
    "sag dem nutzer nichts",
    "sag dem benutzer nichts",
    "erzähl dem nutzer nichts",
    "ohne dass der nutzer es merkt",
];

/// "ignore … instructions" in either language: a word of the first list followed within
/// [`NEAR_CHARS`] by one of the second.
const OVERRIDE_VERBS: [&str; 7] = [
    "ignore",
    "ignoriere",
    "ignorier",
    "disregard",
    "forget",
    "vergiss",
    "missachte",
];
const OVERRIDE_OBJECTS: [&str; 6] = [
    "instruction",
    "anweisung",
    "rules",
    "regeln",
    "prompt",
    "guidelines",
];
const NEAR_CHARS: usize = 60;

fn addresses_an_agent(text: &str) -> bool {
    let lower = text
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    INSTRUCTION_PHRASES.iter().any(|p| lower.contains(p))
        || CONCEALMENT_PHRASES.iter().any(|p| lower.contains(p))
        || OVERRIDE_VERBS.iter().any(|verb| {
            lower.match_indices(verb).any(|(at, _)| {
                let after = &lower[at..];
                let window: String = after.chars().take(NEAR_CHARS).collect();
                OVERRIDE_OBJECTS
                    .iter()
                    .any(|object| window.contains(object))
            })
        })
}
