//! What the final text of a request says when the voice assistant speaks it (spec 078 FR-006, FR-009,
//! research R7): whether it suits being spoken, which language it is in, and whether it declines.
//! Pure functions over the text.

use super::super::scoring::Lang;

/// Whether a final answer suits being spoken (research R7): at most four sentences, no table, list,
/// heading, code, bold or written-out link.
pub fn speech_fit(text: &str) -> bool {
    let text = text.trim();
    let marked = text.contains("**")
        || text.contains('`')
        || text.contains("http://")
        || text.contains("https://")
        || text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with('|')
                || line.starts_with("- ")
                || line.starts_with("* ")
                || line.starts_with('#')
                || line
                    .split_once(". ")
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        });
    !marked && sentence_count(text) <= 4
}

/// Sentences end at `.`, `!` or `?` followed by whitespace or the end of the text.
fn sentence_count(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut in_sentence = false;
    for (i, c) in chars.iter().enumerate() {
        if !c.is_whitespace() {
            in_sentence = true;
        }
        let ends = matches!(c, '.' | '!' | '?')
            && chars.get(i + 1).is_none_or(|next| next.is_whitespace());
        if ends && in_sentence {
            count += 1;
            in_sentence = false;
        }
    }
    count + usize::from(in_sentence)
}

/// The words of a text in lower case, with `’` read as `'`.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('’', "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Short words found in one of the two languages only, separated by spaces.
const GERMAN: &str = "der die das und ist sind nicht ich du dir mir ein eine einen mit von für \
    auf hat habe kein keine uhr heute gestern dein deine wir kann es sie schon noch gibt soll oder \
    aber leider nein";
const ENGLISH: &str =
    "the and is are not you it it's with from for has have no today yesterday your \
    can can't cannot there this that i i'm don't what which should would of to sorry unfortunately";

/// Negations an honest refusal carries, separated by spaces.
const NEGATIONS: &str = "nicht kein keine keinen keiner leider nein nie not no cannot unable \
    unfortunately sorry never";

fn listed(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// The language of a text when its short words say so clearly: at least two of one language and
/// more than twice as many as of the other. `None` for a text too short to tell ("9:41").
// ponytail: a word list for German and English only; it cannot tell a short or mixed answer. A
// language detection crate is the upgrade once the set has more languages.
pub fn answer_language(text: &str) -> Option<Lang> {
    let words = words(text);
    let count = |list: &str| words.iter().filter(|w| listed(list, w)).count();
    let (de, en) = (count(GERMAN), count(ENGLISH));
    if de >= 2 && de > 2 * en {
        Some(Lang::De)
    } else if en >= 2 && en > 2 * de {
        Some(Lang::En)
    } else {
        None
    }
}

/// Whether a text declines or says something is missing, as an honest refusal does ("Das kann ich
/// noch nicht", "I can't set alarms"); a text that only claims it is done does not.
// ponytail: a negation word list; "Kein Problem, erledigt" passes as a refusal. Good enough to catch
// a plain false "done"; a judging model is the upgrade if the set needs more.
pub fn declines(text: &str) -> bool {
    words(text)
        .iter()
        .any(|w| listed(NEGATIONS, w) || w.ends_with("n't"))
}
