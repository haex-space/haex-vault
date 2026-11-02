//! The word list (spec 066 research R22): everything haex says or understands locally, one row per
//! meaning and one column per spoken language, in `lexicon.json`.
//!
//! Row kinds by the prefix of their id: `hear.*` (phrases haex recognizes; a list per language and
//! one `match` rule for all of them), `detect` (frequent short words that tell the languages
//! apart), `say.*` (sentences haex speaks, with `{placeholders}`) and `prompt.*` (instructions to
//! the model). Dates, times and numbers are grammar, not words; they live in `speakable`.
//!
//! Also compiled into builds without the `voice` feature: the stop words of spec 008
//! (`stt::interrupt`) and Whisper's language candidates read this list too.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

const LEXICON_JSON: &str = include_str!("lexicon.json");

/// The language a sentence falls back to when the asked one has no column.
pub const FALLBACK_LANGUAGE: &str = "en";

/// How a `hear.*` row matches an utterance, compared word by word after [`normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    /// The whole utterance is the phrase.
    Whole,
    /// The utterance begins with the phrase.
    Start,
    /// The phrase appears anywhere in the utterance.
    Contains,
}

/// One cell: the phrases of a `hear.*` or `detect` row, or the text of a `say.*` or `prompt.*` row.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Phrases(Vec<String>),
    Text(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "match")]
    pub matching: Option<Match>,
    #[serde(flatten)]
    pub cells: HashMap<String, Cell>,
}

#[derive(Debug, Deserialize)]
struct RawLexicon {
    languages: Vec<String>,
    entries: Vec<Entry>,
}

#[derive(Debug, Default)]
pub struct Lexicon {
    languages: Vec<String>,
    entries: Vec<Entry>,
    /// The phrases of every phrase cell, normalized once: `(id, language)` → word sequences.
    phrases: HashMap<(String, String), Vec<Vec<String>>>,
}

impl Lexicon {
    /// Parses a word list in the format of `lexicon.json`.
    pub fn parse(json: &str) -> Result<Self, String> {
        let raw: RawLexicon =
            serde_json::from_str(json).map_err(|e| format!("parse the word list: {e}"))?;
        let mut phrases = HashMap::new();
        for entry in &raw.entries {
            for (language, cell) in &entry.cells {
                if let Cell::Phrases(list) = cell {
                    let words = list.iter().map(|phrase| words(phrase)).collect();
                    phrases.insert((entry.id.clone(), language.clone()), words);
                }
            }
        }
        Ok(Self {
            languages: raw.languages,
            entries: raw.entries,
            phrases,
        })
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The normalized phrases of row `id` in `language`; empty when either is unknown.
    pub fn phrases(&self, id: &str, language: &str) -> &[Vec<String>] {
        self.phrases
            .get(&(id.to_owned(), language.to_owned()))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Whether `text` matches row `id` in `language` by the row's `match` rule.
    pub fn hears(&self, id: &str, language: &str, text: &str) -> bool {
        self.matched(id, language, text).is_some()
    }

    /// The phrase of row `id` in `language` that `text` matches, as normalized words.
    pub fn matched(&self, id: &str, language: &str, text: &str) -> Option<&[String]> {
        let rule = self.entry(id)?.matching?;
        let said = words(text);
        self.phrases(id, language)
            .iter()
            .find(|phrase| matches(rule, &said, phrase))
            .map(Vec::as_slice)
    }

    /// The sentence of row `id` in `language` (or [`FALLBACK_LANGUAGE`]) with every `{name}`
    /// replaced from `args`; empty when the row has no text.
    pub fn say(&self, id: &str, language: &str, args: &[(&str, &str)]) -> String {
        let Some(entry) = self.entry(id) else {
            return String::new();
        };
        let text = [language, FALLBACK_LANGUAGE]
            .iter()
            .find_map(|code| match entry.cells.get(*code) {
                Some(Cell::Text(text)) => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or_default();
        args.iter().fold(text.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
    }

    /// The language whose `detect` words occur most often in `text`. On a tie, or when no word
    /// occurs, `previous` wins if it is one of the candidates; otherwise the first of them in the
    /// order of `languages`.
    pub fn detect<'a>(&'a self, text: &str, previous: &'a str) -> &'a str {
        let said = words(text);
        let hits: Vec<(&str, usize)> = self
            .languages
            .iter()
            .map(|language| {
                let list = self.phrases("detect", language);
                let count = said
                    .iter()
                    .filter(|word| list.iter().any(|p| p.len() == 1 && &p[0] == *word))
                    .count();
                (language.as_str(), count)
            })
            .collect();
        let best = hits.iter().map(|(_, count)| *count).max().unwrap_or(0);
        let leaders: Vec<&str> = hits
            .iter()
            .filter(|(_, count)| *count == best)
            .map(|(language, _)| *language)
            .collect();
        if leaders.contains(&previous) {
            return previous;
        }
        leaders.first().copied().unwrap_or(previous)
    }
}

/// The word list shipped in the binary. A list that does not parse hears nothing and says
/// nothing; `lexicon_tests` keeps that from shipping.
pub fn lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| {
        Lexicon::parse(LEXICON_JSON).unwrap_or_else(|reason| {
            log::error!("{reason}");
            Lexicon::default()
        })
    })
}

/// The spoken languages, in the order of the word list.
pub fn languages() -> &'static [String] {
    lexicon().languages()
}

/// See [`Lexicon::hears`].
pub fn hears(id: &str, language: &str, text: &str) -> bool {
    lexicon().hears(id, language, text)
}

/// See [`Lexicon::say`].
pub fn say(id: &str, language: &str, args: &[(&str, &str)]) -> String {
    lexicon().say(id, language, args)
}

/// Lower case, typographic apostrophes as `'`, hyphens and every other character that is neither
/// a letter, a digit nor an apostrophe as a space, runs of spaces as one.
pub fn normalize(text: &str) -> String {
    words(text).join(" ")
}

/// [`normalize`]d text split into words.
pub fn words(text: &str) -> Vec<String> {
    let cleaned: String = text
        .chars()
        .map(|c| match c {
            '’' | '‘' | '`' | '´' | 'ʼ' => '\'',
            c if c.is_alphanumeric() || c == '\'' => c,
            _ => ' ',
        })
        .collect::<String>()
        .to_lowercase();
    cleaned
        .split_whitespace()
        .map(|word| word.trim_matches('\''))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

fn matches(rule: Match, said: &[String], phrase: &[String]) -> bool {
    if phrase.is_empty() {
        return false;
    }
    match rule {
        Match::Whole => said == phrase,
        Match::Start => said.starts_with(phrase),
        Match::Contains => said.windows(phrase.len()).any(|window| window == phrase),
    }
}
