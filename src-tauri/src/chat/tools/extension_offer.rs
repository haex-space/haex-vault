//! The matched offer (spec 018, research R6, FR-006, FR-007): before the first step holzi compares
//! the message with every offerable extension tool and offers up to three that fit, so a small
//! model does not have to search for them. Deterministic and without a model: words of the
//! message against the tool's titles, description and examples, each word weighted by how rare
//! it is among all tool texts (IDF). Naming the extension offers its tools.

use std::collections::{BTreeSet, HashSet};

use super::offer::word_matches;

/// The most tools a message adds to the first offer.
pub const MAX_MATCHED: usize = 3;
/// A tool needs this many words of the message in its texts, unless the message names its
/// extension: one shared word ("heute", "Bad") is too weak a sign.
const MIN_WORDS: usize = 2;
/// The least weighted score of a matched tool. Each matching word counts at least 1.
const MIN_SCORE: f64 = 2.0;
/// Puts the tools of an extension the message names ahead of every other match.
const NAME_BONUS: f64 = 1000.0;

/// Words that never carry the request (de/en).
const STOP_WORDS: &[&str] = &[
    "a", "about", "am", "an", "and", "are", "auch", "auf", "aus", "bei", "bin", "bist", "bitte",
    "can", "could", "dann", "das", "dass", "dein", "deine", "dem", "den", "denn", "der", "des",
    "did", "die", "dir", "do", "does", "doch", "du", "ein", "eine", "einem", "einen", "einer",
    "er", "es", "for", "from", "gib", "gibt", "habe", "haben", "hast", "hat", "have", "how", "i",
    "ich", "ihr", "im", "in", "is", "ist", "it", "ja", "kann", "kannst", "kein", "keine", "me",
    "mein", "meine", "meiner", "mich", "mir", "mit", "muss", "my", "nach", "nein", "nicht", "noch",
    "nur", "of", "oder", "on", "please", "schon", "sie", "sind", "so", "the", "this", "to", "uns",
    "und", "von", "war", "was", "welche", "welcher", "welches", "wenn", "wer", "what", "when",
    "where", "which", "who", "wie", "wir", "wo", "you", "your", "zu", "zum", "zur",
];

/// What the matched offer reads about one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEntry {
    /// The model-facing name.
    pub tool_name: String,
    pub extension_title: String,
    /// Titles, description and example sentences.
    pub texts: Vec<String>,
}

/// Lowercase words; unlike `offer.rs`, letters beyond ASCII belong to the word ("Einträge").
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

fn content_words(text: &str) -> BTreeSet<String> {
    words(text)
        .filter(|word| !STOP_WORDS.contains(&word.as_str()))
        .collect()
}

fn mentions(known: &HashSet<String>, word: &str) -> bool {
    known.iter().any(|k| word_matches(word, k))
}

/// The message names the extension: every content word of its title occurs in the message.
fn names_extension(title: &str, message: &BTreeSet<String>) -> bool {
    let title = content_words(title);
    !title.is_empty()
        && title
            .iter()
            .all(|t| message.iter().any(|m| word_matches(m, t)))
}

/// The model-facing names of up to [`MAX_MATCHED`] tools that fit `message`, best first, ties
/// by name.
pub fn matched_offer(message: &str, entries: &[MatchEntry]) -> Vec<String> {
    let query = content_words(message);
    if query.is_empty() || entries.is_empty() {
        return Vec::new();
    }
    let docs: Vec<HashSet<String>> = entries
        .iter()
        .map(|entry| {
            entry
                .texts
                .iter()
                .flat_map(|text| words(text))
                .chain(words(&entry.tool_name))
                .collect()
        })
        .collect();
    let total = entries.len() as f64;
    let weight = |word: &str| {
        let containing = docs.iter().filter(|doc| mentions(doc, word)).count().max(1);
        1.0 + (total / containing as f64).ln()
    };
    let mut scored: Vec<(f64, &str)> = entries
        .iter()
        .zip(&docs)
        .filter_map(|(entry, doc)| {
            let matched: Vec<&String> = query.iter().filter(|w| mentions(doc, w)).collect();
            let score: f64 = matched.iter().map(|w| weight(w)).sum();
            let named = names_extension(&entry.extension_title, &query);
            let fits = matched.len() >= MIN_WORDS && score >= MIN_SCORE;
            (named || fits).then(|| {
                let bonus = if named { NAME_BONUS } else { 0.0 };
                (score + bonus, entry.tool_name.as_str())
            })
        })
        .collect();
    scored.sort_by(|(left, left_name), (right, right_name)| {
        right
            .total_cmp(left)
            .then_with(|| left_name.cmp(right_name))
    });
    scored
        .into_iter()
        .take(MAX_MATCHED)
        .map(|(_, name)| name.to_owned())
        .collect()
}
