//! Splits a written-out sentence into parts a speech model speaks one after another (spec 081
//! FR-010, research R5). Long input makes an autoregressive model drift, and Qwen3-TTS hands out
//! audio only at the end of a call, so a long sentence would also start late.
//!
//! A part ends at the most natural place: after punctuation inside the sentence, else before a
//! conjunction, else between two words. Words joined by [`PROTECTED_SPACE`] (one written-out
//! number, time, date, amount or unit) are never split. Parts are balanced: 30 words become two
//! parts of about 15, not 20 and 10.

use super::spoken::PROTECTED_SPACE;

/// Punctuation after which a part may end.
const BREAKS: [char; 7] = [',', ';', ':', '–', '—', ')', '…'];

/// Words before which a part may end, per language.
fn conjunctions(language: &str) -> &'static [&'static str] {
    match language {
        "de" => &[
            "und", "oder", "aber", "denn", "weil", "dass", "wenn", "sondern", "doch", "damit",
            "obwohl", "sodass", "während", "bevor", "nachdem", "als", "bis",
        ],
        "fr" => &[
            "et", "ou", "mais", "car", "parce", "que", "quand", "si", "donc", "puis", "lorsque",
            "pendant",
        ],
        "es" => &[
            "y", "o", "pero", "porque", "que", "cuando", "si", "aunque", "mientras", "pues", "sino",
        ],
        "it" => &[
            "e", "o", "ma", "perché", "che", "quando", "se", "anche", "mentre", "poiché", "però",
        ],
        "pt" => &[
            "e", "ou", "mas", "porque", "que", "quando", "se", "embora", "enquanto", "pois",
            "porém",
        ],
        _ => &[
            "and", "or", "but", "because", "that", "when", "if", "while", "although", "so",
            "unless", "until", "before", "after",
        ],
    }
}

/// How many words `token` counts: a protected group counts every word in it.
fn weight(token: &str) -> usize {
    1 + token.matches(PROTECTED_SPACE).count()
}

/// How good it is to end a part after `token`, with `next` after it: 3 after punctuation, 2
/// before a conjunction, 1 anywhere else between words.
fn break_score(token: &str, next: &str, language: &str) -> u32 {
    if token.ends_with(BREAKS) {
        return 3;
    }
    let next = next
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if conjunctions(language).contains(&next.as_str()) {
        return 2;
    }
    1
}

/// Splits `text` (as `tts::spoken::spoken_protected` writes it) into parts of at most
/// `max_words` words, in order. Protected groups stay joined, so a part can be split again; the
/// engine gets [`plain`] text. A protected group longer than `max_words` stays one part.
pub fn split(text: &str, language: &str, max_words: usize) -> Vec<String> {
    let tokens: Vec<&str> = text.split(' ').filter(|t| !t.is_empty()).collect();
    let total: usize = tokens.iter().map(|t| weight(t)).sum();
    let max_words = max_words.max(1);
    let mut parts = Vec::new();
    if total <= max_words {
        if !tokens.is_empty() {
            parts.push(join(&tokens));
        }
        return parts;
    }
    let mut start = 0;
    let mut left = total;
    while start < tokens.len() {
        if left <= max_words {
            parts.push(join(&tokens[start..]));
            break;
        }
        let parts_left = left.div_ceil(max_words);
        let target = left.div_ceil(parts_left);
        let end = best_break(&tokens[start..], target, max_words, language) + start;
        left -= tokens[start..end].iter().map(|t| weight(t)).sum::<usize>();
        parts.push(join(&tokens[start..end]));
        start = end;
    }
    parts
}

/// Where the next part of `tokens` ends (exclusive index): the best break among those that keep
/// the part between half the target and `max_words` words, preferring the higher score and then
/// the one nearest `target`.
fn best_break(tokens: &[&str], target: usize, max_words: usize, language: &str) -> usize {
    let mut best: Option<(u32, usize, usize)> = None; // (score, distance to target, end)
    let mut words = 0;
    for i in 0..tokens.len().saturating_sub(1) {
        words += weight(tokens[i]);
        if words > max_words {
            break;
        }
        if words * 2 < target {
            continue;
        }
        let score = break_score(tokens[i], tokens[i + 1], language);
        let distance = words.abs_diff(target);
        let better = match best {
            None => true,
            Some((s, d, _)) => score > s || (score == s && distance < d),
        };
        if better {
            best = Some((score, distance, i + 1));
        }
    }
    // Nothing fits (a protected group longer than a part): end after the first token.
    best.map_or(1, |(_, _, end)| end)
}

fn join(tokens: &[&str]) -> String {
    tokens.join(" ")
}

/// `part` with its protected groups written with plain spaces, as a speech model reads it.
pub fn plain(part: &str) -> String {
    part.replace(PROTECTED_SPACE, " ")
}
