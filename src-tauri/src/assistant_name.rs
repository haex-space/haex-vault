//! The name of the assistant, written and spoken (design 2026-11-16).
//!
//! One name per vault, the same on every device: the vault preference `assistant.name`. It has two
//! forms. The *written* form is what is shown, what the model is told and what the model writes;
//! the *spoken* form is how it is said and heard (speech output, wake word). The default is
//! "Haex", said "Hex". The harness keeps the spelling: [`AssistantName::heard_in`] turns what
//! speech recognition heard into the written form, [`AssistantName::spelled_in`] fixes the case in
//! what is written; [`AssistantName::spoken_in`]
//! turns the written form into the spoken one before a voice reads it.
//!
//! Replacements are whole-word, case-insensitive and exact (no fuzzy matching). A match that is
//! part of a compound or a path ("Hex-Code", "haex-pass", "#hex", "a/haex") and anything in code
//! is left alone, because the spoken default "Hex" is also a word of its own.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const PREF_KEY: &str = "assistant.name";

const MAX_CHARS: usize = 24;
const MAX_WORDS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("the name is empty")]
    Empty,
    #[error("the name is longer than {MAX_CHARS} characters")]
    TooLong,
    #[error("the name has more than {MAX_WORDS} words")]
    TooManyWords,
    #[error("the name has a character that is not a letter, digit or hyphen: {0:?}")]
    BadCharacter(char),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AssistantName {
    written: String,
    spoken: String,
}

impl Default for AssistantName {
    fn default() -> Self {
        Self {
            written: "Haex".to_owned(),
            spoken: "Hex".to_owned(),
        }
    }
}

/// `text` as a name: trimmed, words separated by one space; one or two words of letters, digits
/// and hyphens, at most [`MAX_CHARS`] characters.
fn checked(text: &str) -> Result<String, NameError> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Err(NameError::Empty);
    }
    if words.len() > MAX_WORDS {
        return Err(NameError::TooManyWords);
    }
    let name = words.join(" ");
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_alphanumeric() || *c == '-' || *c == ' '))
    {
        return Err(NameError::BadCharacter(bad));
    }
    if name.chars().count() > MAX_CHARS {
        return Err(NameError::TooLong);
    }
    Ok(name)
}

impl AssistantName {
    /// An empty `spoken` is said as it is written.
    pub fn new(written: &str, spoken: &str) -> Result<Self, NameError> {
        let written = checked(written)?;
        let spoken = if spoken.trim().is_empty() {
            written.clone()
        } else {
            checked(spoken)?
        };
        Ok(Self { written, spoken })
    }

    /// What is stored in the preference; the default when there is nothing or nothing usable.
    pub fn from_stored(stored: Option<&str>) -> Self {
        stored
            .and_then(|text| serde_json::from_str::<Self>(text).ok())
            .and_then(|name| Self::new(&name.written, &name.spoken).ok())
            .unwrap_or_default()
    }

    pub fn written(&self) -> &str {
        &self.written
    }

    pub fn spoken(&self) -> &str {
        &self.spoken
    }

    /// The sentence of the system prompt: the model only ever sees the written form.
    pub fn prompt_line(&self) -> String {
        format!(
            "The assistant is called {0}. When you or the user mention the assistant, write its name exactly like this: {0}.",
            self.written
        )
    }

    /// `text` as speech recognition heard it: the spoken form and any case of the written form
    /// become the written form. Only for what was *said*; a word like "Hex" is not the assistant
    /// in text that someone wrote.
    pub fn heard_in(&self, text: &str) -> String {
        replace_names(text, &[&self.spoken, &self.written], &self.written)
    }

    /// `text` as someone or the model wrote it: any case of the written form becomes the written
    /// form. The spoken form is left alone, since it can be a word of its own ("Hex").
    pub fn spelled_in(&self, text: &str) -> String {
        replace_names(text, &[&self.written], &self.written)
    }

    /// `text` with the written form replaced by the spoken form, for a voice to read.
    pub fn spoken_in(&self, text: &str) -> String {
        replace_names(text, &[&self.written], &self.spoken)
    }
}

/// Mirrors the open vault's name for the places that have no database at hand (the speech output).
static CURRENT: RwLock<Option<AssistantName>> = RwLock::new(None);

/// The name of the open vault; the default before one is read.
pub fn current() -> AssistantName {
    CURRENT
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .unwrap_or_default()
}

pub fn set_current(name: AssistantName) {
    *CURRENT
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(name);
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Whether `c` joins a match to a word, a compound, a code or a path around it.
fn joins(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '/' | '\\' | '@' | '#')
}

/// Whether the match at `chars[start..end]` stands alone: not inside a word, not hyphenated to
/// another word ("Hex-Code", "haex-pass").
fn stands_alone(chars: &[char], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).map(|i| chars[i]);
    let after = chars.get(end).copied();
    let hyphenated = |dash: Option<char>, beyond: Option<char>| {
        dash == Some('-') && beyond.is_some_and(char::is_alphanumeric)
    };
    !before.is_some_and(joins)
        && !after.is_some_and(joins)
        && !hyphenated(before, start.checked_sub(2).map(|i| chars[i]))
        && !hyphenated(after, chars.get(end + 1).copied())
}

/// How many characters of `chars[at..]` match `needle` (words in any whitespace run), if all do.
fn matched(chars: &[char], at: usize, needle: &str) -> Option<usize> {
    let mut i = at;
    for (n, word) in needle.split(' ').enumerate() {
        if n > 0 {
            let spaces = chars[i..].iter().take_while(|c| c.is_whitespace()).count();
            if spaces == 0 {
                return None;
            }
            i += spaces;
        }
        for expected in word.chars() {
            if !chars.get(i).is_some_and(|c| same(*c, expected)) {
                return None;
            }
            i += 1;
        }
    }
    Some(i - at)
}

/// `text` with every stand-alone `needles` outside code replaced by `to`. Code is what lies between
/// backtick runs of the same length (`code`, ``code``, a fence), as in Markdown.
fn replace_names(text: &str, needles: &[&str], to: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    // The length of the backtick run that opened the code the loop is in.
    let mut code_run: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            let run = chars[i..].iter().take_while(|c| **c == '`').count();
            match code_run {
                None => code_run = Some(run),
                Some(open) if open == run => code_run = None,
                Some(_) => {}
            }
            out.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        if code_run.is_none() {
            // The longest needle first, so "Anna Lena" wins over "Anna".
            let found = needles
                .iter()
                .filter_map(|needle| matched(&chars, i, needle))
                .max()
                .filter(|len| stands_alone(&chars, i, i + len));
            if let Some(len) = found {
                out.push_str(to);
                i += len;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
#[path = "assistant_name_tests.rs"]
mod tests;
