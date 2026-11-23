//! The search over the pages (spec 097 FR-028, research R4): the words of a query, with case and
//! accents folded, matched against title, description and text. A hit in the title counts most,
//! then one in the description, then one in the text. At 1,000 pages of 10,000 characters this is
//! a scan of 10 MB; a full-text index is the step if it gets too slow.
//!
//! A pure function over a slice of pages, so the store and the everyday measurement share it.

use std::cmp::Reverse;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

/// The most results a search gives.
pub const SEARCH_RESULTS: usize = 10;
/// The longest excerpt of a result.
pub const EXCERPT_CHARS: usize = 200;
/// How much of the text before the first hit an excerpt shows.
const EXCERPT_LEAD_CHARS: usize = 50;

const TITLE_WEIGHT: u32 = 100;
const DESCRIPTION_WEIGHT: u32 = 10;
const BODY_WEIGHT: u32 = 1;

/// A page as the search reads it.
#[derive(Debug, Clone, Copy)]
pub struct SearchPage<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub body: &'a str,
    /// `max(created_at, changed_at)`: ties go to the page changed last.
    pub changed_at: i64,
}

/// One result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub description: String,
    pub excerpt: String,
}

/// The pages matching `query`, best first, at most [`SEARCH_RESULTS`].
pub fn search(pages: &[SearchPage<'_>], query: &str) -> Vec<Hit> {
    let words: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|word| fold(word).0)
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(u32, &SearchPage<'_>, Option<usize>)> = pages
        .iter()
        .filter_map(|page| {
            let title = fold(page.title).0;
            let description = fold(page.description).0;
            let (body, origin) = fold(page.body);
            let mut score = 0;
            let mut first_in_body: Option<usize> = None;
            for word in &words {
                let in_body = find(&body, word);
                score += if find(&title, word).is_some() {
                    TITLE_WEIGHT
                } else if find(&description, word).is_some() {
                    DESCRIPTION_WEIGHT
                } else if in_body.is_some() {
                    BODY_WEIGHT
                } else {
                    0
                };
                if let Some(at) = in_body {
                    let at = origin[at];
                    first_in_body = Some(first_in_body.map_or(at, |first| first.min(at)));
                }
            }
            (score > 0).then_some((score, page, first_in_body))
        })
        .collect();
    scored.sort_by_key(|(score, page, _)| (Reverse(*score), Reverse(page.changed_at), page.id));
    scored
        .into_iter()
        .take(SEARCH_RESULTS)
        .map(|(_, page, first)| Hit {
            id: page.id.to_owned(),
            title: page.title.to_owned(),
            description: page.description.to_owned(),
            excerpt: excerpt(page.body, first),
        })
        .collect()
}

/// `text` lowercase, without accents, `ß` as `ss`; and for each folded character the index of the
/// character of `text` it came from.
fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::with_capacity(text.len());
    let mut origin = Vec::with_capacity(text.len());
    for (index, c) in text.chars().enumerate() {
        for lower in c.to_lowercase() {
            if lower == 'ß' {
                folded.extend(['s', 's']);
                origin.extend([index, index]);
                continue;
            }
            for part in std::iter::once(lower)
                .nfd()
                .filter(|c| !is_combining_mark(*c))
            {
                folded.push(part);
                origin.push(index);
            }
        }
    }
    (folded, origin)
}

/// Where `word` first stands in `text`.
fn find(text: &[char], word: &[char]) -> Option<usize> {
    if word.len() > text.len() {
        return None;
    }
    text.windows(word.len()).position(|window| window == word)
}

/// Up to [`EXCERPT_CHARS`] characters of `body` around the character `first`, or from its start;
/// line breaks become spaces.
fn excerpt(body: &str, first: Option<usize>) -> String {
    let start = first.map_or(0, |at| at.saturating_sub(EXCERPT_LEAD_CHARS));
    body.chars()
        .skip(start)
        .take(EXCERPT_CHARS)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
