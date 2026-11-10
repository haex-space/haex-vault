//! What the assistant recognizes in an utterance without asking the model, and the short sentences
//! it says on its own (spec 066 T042, FR-014, FR-017, FR-019, research R12). All words come from
//! the word list (`lexicon`).

use super::lexicon::{self, lexicon};
use crate::stt::interrupt::{match_interrupt, InterruptCommand};

/// Whether `text` ends the conversation: an end phrase (row `hear.end`) as the whole utterance,
/// with nothing around it but small additions such as "danke" or "erstmal" (row `hear.endExtra`).
/// "Danke, das war's erstmal" ends; "das war's noch nicht alles" and a request that merely
/// contains an end phrase do not. "Danke" alone does not end either.
pub fn is_end(language: &str, text: &str) -> bool {
    let mut said = lexicon::words(text);
    let ends = lexicon().phrases("hear.end", language);
    let extras = lexicon().phrases("hear.endExtra", language);
    let removed_end = remove_all(&mut said, ends);
    remove_all(&mut said, extras);
    removed_end && said.is_empty()
}

/// A stop word of spec 008 as the whole utterance, in any spoken language (FR-017). The turn and
/// the speech stop without asking the model; the conversation stays open.
pub fn stop(text: &str) -> Option<InterruptCommand> {
    match_interrupt(text)
}

/// Whether an utterance holds nothing to answer: no words, or only hesitation sounds (row
/// `hear.filler`). It gets a request to repeat and no model call (FR-019).
pub fn is_empty(language: &str, text: &str) -> bool {
    let mut said = lexicon::words(text);
    remove_all(&mut said, lexicon().phrases("hear.filler", language));
    said.is_empty()
}

/// The request to repeat (FR-019).
pub fn repeat(language: &str) -> String {
    lexicon::say("say.repeat", language, &[])
}

/// A chat turn already runs in a window; the assistant waits (research R12).
pub fn busy(language: &str) -> String {
    lexicon::say("say.busy", language, &[])
}

/// The other turn did not finish within the wait (research R12).
pub fn busy_gave_up(language: &str) -> String {
    lexicon::say("say.busyGaveUp", language, &[])
}

/// Said once per conversation when the model is a delegate: its haex tools depend on its agent
/// profile (spec 021 US6).
pub fn delegate_notice(language: &str) -> String {
    lexicon::say("say.delegate", language, &[])
}

/// What haex says when a conversation ends by an end phrase.
pub fn goodbye(language: &str) -> String {
    lexicon::say("say.goodbye", language, &[])
}

/// Removes every occurrence of any of `phrases` from `said`, longest phrases first so "danke
/// schön" goes as one. Returns whether anything was removed.
fn remove_all(said: &mut Vec<String>, phrases: &[Vec<String>]) -> bool {
    let mut sorted: Vec<&Vec<String>> = phrases.iter().filter(|p| !p.is_empty()).collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let mut removed = false;
    let mut i = 0;
    while i < said.len() {
        match sorted
            .iter()
            .find(|phrase| said[i..].starts_with(phrase.as_slice()))
        {
            Some(phrase) => {
                said.drain(i..i + phrase.len());
                removed = true;
            }
            None => i += 1,
        }
    }
    removed
}

/// `text` without the wake word it starts with (row `hear.wake`, FR-011): the listener after the
/// wake word starts with what was said just before, so "Hey Hex, wie spät ist es?" arrives whole.
/// What stands between the wake word and the request: spaces and punctuation, a dash that is
/// set apart ("Hex - open mail"). A sign that belongs to the request stays: "-5 plus 3", "$5", "(a)".
fn skip_separators(text: &str) -> &str {
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let mut chars = trimmed.chars();
        let Some(first) = chars.next() else {
            return trimmed;
        };
        let separator = matches!(
            first,
            ',' | '.' | '!' | '?' | ':' | ';' | '…' | '"' | '“' | '”' | '„' | '\''
        ) || (matches!(first, '-' | '–' | '—')
            && chars.next().is_none_or(char::is_whitespace));
        if !separator {
            return trimmed;
        }
        rest = &trimmed[first.len_utf8()..];
    }
}

/// Empty when the wake word was all that was said.
pub fn without_wake_word(language: &str, text: &str) -> String {
    let Some(phrase) = lexicon().matched("hear.wake", language, text) else {
        return text.trim().to_owned();
    };
    // Skip as many words of the original text as the phrase has, then what separates them.
    let mut rest = text;
    for _ in 0..phrase.len() {
        let start = rest
            .find(|c: char| c.is_alphanumeric())
            .unwrap_or(rest.len());
        rest = &rest[start..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '\''))
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    let rest = skip_separators(rest);
    let mut chars = rest.trim_end().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
