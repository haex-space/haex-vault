//! The language of each sentence of an answer (spec 066 FR-022d, operator 2026-11-07): an answer in
//! the conversation's language may quote a passage in another one (Hamlet in English, then
//! Schlegel's German), and each passage is spoken in its own language. The word list's `detect`
//! words decide sentence by sentence, with a bias to stay: a sentence without clues, or with only
//! one clue against words of the current language, keeps the language of the sentence before.

use super::lexicon::lexicon;

/// Follows the language through the sentences of one answer.
#[derive(Debug, Clone)]
pub struct PassageLanguage {
    base: String,
    current: String,
}

impl PassageLanguage {
    /// Starts in `base`, the language of the conversation or of the whole text.
    pub fn new(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            current: base.to_owned(),
        }
    }

    /// The language of `sentence`. It changes to another language only when that language alone
    /// has the most clues, more than the current one, and either at least two of them or the
    /// current language none.
    pub fn of(&mut self, sentence: &str) -> &str {
        let hits = lexicon().detect_hits(sentence);
        let count = |language: &str| {
            hits.iter()
                .find(|(l, _)| *l == language)
                .map_or(0, |(_, n)| *n)
        };
        let best = hits.iter().map(|(_, n)| *n).max().unwrap_or(0);
        let mut leaders = hits.iter().filter(|(_, n)| *n == best && best > 0);
        if let (Some((leader, n)), None) = (leaders.next(), leaders.next()) {
            let current = count(&self.current);
            if *leader != self.current && *n > current && (*n >= 2 || current == 0) {
                self.current = (*leader).to_owned();
            }
        }
        &self.current
    }

    /// The answer is over: the next one starts in the base language again.
    pub fn reset(&mut self) {
        self.current.clone_from(&self.base);
    }
}
