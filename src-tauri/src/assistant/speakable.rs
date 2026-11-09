//! Turns the model's answer into what haex says (spec 066 FR-022, research R10): no formatting
//! marks; a table, a code block or a written-out link becomes a short pointer to the chat; ISO
//! dates and times are spoken as words relative to today ("morgen um zehn Uhr"). An answer is
//! spoken to its end, however long (operator 2026-11-07): the user interrupts instead of being
//! asked whether to go on. The chat and the transcript keep the full text.
//!
//! Date and time words are grammar, so they are code with one branch per spoken language
//! (`calendar_words`, `number_words`); the sentences come from the word list.

use super::calendar_words::{day_phrase, time_phrase};
use super::lexicon::{self, FALLBACK_LANGUAGE};
use super::sentences::{fence_closer, fence_opener, Fence, Sentences};

pub use crate::local_time::{Civil, Clock};

/// [`Speech`] over a complete text: the sentences to say, in order.
pub fn speak_all(text: &str, language: &str, clock: Clock) -> Vec<String> {
    let mut speech = Speech::new(language, clock);
    let mut sentences = speech.push(text);
    sentences.extend(speech.finish());
    sentences
}

/// The streamed answer as spoken sentences: token deltas in, sentences to say out.
#[derive(Debug)]
pub struct Speech {
    sentences: Sentences,
    lines: Speakable,
}

impl Speech {
    pub fn new(language: &str, clock: Clock) -> Self {
        Self {
            sentences: Sentences::new(),
            lines: Speakable::new(language, clock),
        }
    }

    /// Adds a token delta; returns the sentences to say now.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        let pieces = self.sentences.push(delta);
        self.admit_all(pieces)
    }

    /// The answer is complete; returns the last sentences to say.
    pub fn finish(&mut self) -> Vec<String> {
        let pieces = self.sentences.finish();
        self.admit_all(pieces)
    }

    fn admit_all(&mut self, pieces: Vec<String>) -> Vec<String> {
        pieces
            .into_iter()
            .filter_map(|piece| self.lines.line(&piece))
            .collect()
    }
}

/// One sentence or line of the answer at a time, with the block it is in (code, table).
#[derive(Debug)]
pub struct Speakable {
    language: String,
    clock: Clock,
    in_fence: Option<Fence>,
    in_table: bool,
}

impl Speakable {
    pub fn new(language: &str, clock: Clock) -> Self {
        let language = if lexicon::languages().iter().any(|l| l == language) {
            language
        } else {
            FALLBACK_LANGUAGE
        };
        Self {
            language: language.to_owned(),
            clock,
            in_fence: None,
            in_table: false,
        }
    }

    /// What to say for `piece`, or `None` when nothing is said for it (a code line, the rest of a
    /// table, a rule, an empty line).
    pub fn line(&mut self, piece: &str) -> Option<String> {
        if let Some(fence) = self.in_fence {
            if fence_closer(piece, fence) {
                self.in_fence = None;
            }
            return None;
        }
        if let Some(fence) = fence_opener(piece) {
            self.in_fence = Some(fence);
            self.in_table = false;
            return Some(lexicon::say("say.pointer.code", &self.language, &[]));
        }
        let piece = piece.trim();
        if piece.starts_with('|') {
            if self.in_table {
                return None;
            }
            self.in_table = true;
            return Some(lexicon::say("say.pointer.table", &self.language, &[]));
        }
        self.in_table = false;
        if is_rule(piece) {
            return None;
        }
        let text = strip_line_marks(piece);
        let text = strip_inline_marks(&text);
        let text = replace_urls(&text, &lexicon::say("say.linkInline", &self.language, &[]));
        let text = speak_dates(&text, &self.language, self.clock);
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!text.is_empty() && text.chars().any(char::is_alphanumeric)).then_some(text)
    }
}

fn is_rule(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3
        && (marks.chars().all(|c| c == '-')
            || marks.chars().all(|c| c == '*')
            || marks.chars().all(|c| c == '_'))
}

/// Headings, quotes, list markers and task boxes at the start of a line.
fn strip_line_marks(line: &str) -> String {
    let mut rest = line.trim_start();
    loop {
        let before = rest;
        rest = rest.trim_start_matches('#').trim_start();
        rest = rest.trim_start_matches('>').trim_start();
        for marker in ["- ", "* ", "+ ", "• "] {
            if let Some(stripped) = rest.strip_prefix(marker) {
                rest = stripped.trim_start();
            }
        }
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && digits <= 3 {
            let after = &rest[digits..];
            if let Some(stripped) = after
                .strip_prefix(". ")
                .or_else(|| after.strip_prefix(") "))
            {
                rest = stripped.trim_start();
            } else if after == "." {
                rest = "";
            }
        }
        for box_mark in ["[ ] ", "[x] ", "[X] "] {
            if let Some(stripped) = rest.strip_prefix(box_mark) {
                rest = stripped;
            }
        }
        if rest == before {
            return rest.to_owned();
        }
    }
}

/// Links and images as their text, inline code as its content, emphasis marks removed.
fn strip_inline_marks(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // [text](url) and ![alt](url)
        if c == '[' || (c == '!' && chars.get(i + 1) == Some(&'[')) {
            let open = if c == '!' { i + 1 } else { i };
            if let Some((label, end)) = link_at(&chars, open) {
                out.push_str(&label);
                i = end;
                continue;
            }
        }
        match c {
            '`' | '*' => {}
            '~' if chars.get(i + 1) == Some(&'~') => i += 1,
            '_' => {
                let prev = i.checked_sub(1).and_then(|p| chars.get(p));
                let next = chars.get(i + 1);
                let inside_word = prev.is_some_and(|p| p.is_alphanumeric())
                    && next.is_some_and(|n| n.is_alphanumeric());
                if inside_word {
                    out.push('_');
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// `[label](target)` starting at `open`: the label and the index after the closing parenthesis.
fn link_at(chars: &[char], open: usize) -> Option<(String, usize)> {
    let close = open + chars[open..].iter().position(|&c| c == ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = close + 1 + chars[close + 1..].iter().position(|&c| c == ')')?;
    Some((chars[open + 1..close].iter().collect(), end + 1))
}

/// Every written-out web address becomes `pointer`; punctuation right after it stays.
fn replace_urls(text: &str, pointer: &str) -> String {
    text.split(' ')
        .map(|word| {
            let core = word.trim_start_matches(['(', '<', '"', '\'']);
            let lower = core.to_lowercase();
            if lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("www.")
            {
                let tail_len = core
                    .chars()
                    .rev()
                    .take_while(|c| {
                        matches!(
                            c,
                            '.' | ',' | ';' | ':' | '!' | '?' | ')' | '>' | '"' | '\''
                        )
                    })
                    .map(char::len_utf8)
                    .sum::<usize>();
                let prefix = &word[..word.len() - core.len()];
                format!("{prefix}{pointer}{}", &core[core.len() - tail_len..])
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A date, a time or both found in the text.
#[derive(Debug, Clone, Copy)]
enum Found {
    Date(Civil),
    DateTime(Civil),
    Time { hour: u32, minute: u32 },
}

/// Replaces ISO dates ("2026-10-29"), ISO date-times ("2026-10-29T10:00", with an optional offset)
/// and clock times ("10:30") by words. A preposition right before a date is dropped, because the
/// spoken form brings its own ("am 2026-10-29" → "morgen").
fn speak_dates(text: &str, language: &str, clock: Clock) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let boundary = i == 0
            || !(chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], ':' | '.' | '-' | '/'));
        if boundary {
            if let Some((found, end)) = parse_at(&chars, i, clock) {
                let mut end = end;
                let words = match found {
                    Found::Date(date) => {
                        drop_preposition(&mut out, language);
                        day_phrase(language, date, clock.now)
                    }
                    Found::DateTime(at) => {
                        drop_preposition(&mut out, language);
                        format!(
                            "{} {}",
                            day_phrase(language, at, clock.now),
                            time_phrase(language, at.hour, at.minute, true)
                        )
                    }
                    Found::Time { hour, minute } => {
                        if language == "de" {
                            let rest: String = chars[end..].iter().collect();
                            if rest.starts_with(" Uhr") {
                                end += " Uhr".len();
                            }
                        }
                        time_phrase(language, hour, minute, false)
                    }
                };
                out.push_str(&words);
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn digits(chars: &[char], at: usize, n: usize) -> Option<u32> {
    let slice = chars.get(at..at + n)?;
    if !slice.iter().all(char::is_ascii_digit) {
        return None;
    }
    slice.iter().collect::<String>().parse().ok()
}

fn not_digit(chars: &[char], at: usize) -> bool {
    chars.get(at).is_none_or(|c| !c.is_ascii_digit())
}

/// `HH:MM` (or `H:MM`), optionally with `:SS`, at `at`: hour, minute and the index after it.
fn time_at(chars: &[char], at: usize) -> Option<(u32, u32, usize)> {
    let (hour, after_hour) = match (digits(chars, at, 2), digits(chars, at, 1)) {
        (Some(h), _) => (h, at + 2),
        (None, Some(h)) => (h, at + 1),
        _ => return None,
    };
    if chars.get(after_hour) != Some(&':') {
        return None;
    }
    let minute = digits(chars, after_hour + 1, 2)?;
    let mut end = after_hour + 3;
    if !not_digit(chars, end) {
        return None;
    }
    if chars.get(end) == Some(&':')
        && digits(chars, end + 1, 2).is_some()
        && not_digit(chars, end + 3)
    {
        end += 3;
        // Fractional seconds; a period without a digit after it ends the sentence.
        if chars.get(end) == Some(&'.') && !not_digit(chars, end + 1) {
            end += 1 + chars[end + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
        }
    }
    (hour < 24 && minute < 60).then_some((hour, minute, end))
}

fn parse_at(chars: &[char], at: usize, clock: Clock) -> Option<(Found, usize)> {
    if let (Some(year), Some('-'), Some(month), Some('-'), Some(day)) = (
        digits(chars, at, 4),
        chars.get(at + 4).copied(),
        digits(chars, at + 5, 2),
        chars.get(at + 7).copied(),
        digits(chars, at + 8, 2),
    ) {
        let date = Civil::new(year as i32, month, day, 0, 0);
        if !date.valid() || !not_digit(chars, at + 10) {
            return None;
        }
        let sep = at + 10;
        if matches!(chars.get(sep), Some('T' | 't' | ' ')) {
            if let Some((hour, minute, mut end)) = time_at(chars, sep + 1) {
                if digits(chars, sep + 1, 2).is_some() {
                    let local = Civil::new(date.year, date.month, date.day, hour, minute);
                    let at_local = match offset_at(chars, end) {
                        Some((offset, after)) => {
                            end = after;
                            Civil::from_minutes(
                                local.minutes() - i64::from(offset)
                                    + i64::from(clock.utc_offset_minutes),
                            )
                        }
                        None => local,
                    };
                    return Some((Found::DateTime(at_local), end));
                }
            }
        }
        return Some((Found::Date(date), sep));
    }
    let (hour, minute, end) = time_at(chars, at)?;
    // "3:2" or a ratio like "16:9" is no clock time; a time has two minute digits (checked) and
    // stands on its own.
    if chars.get(end).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    Some((Found::Time { hour, minute }, end))
}

/// `Z`, `+HH:MM`, `-HH:MM` or `+HHMM` at `at`: the offset in minutes and the index after it.
fn offset_at(chars: &[char], at: usize) -> Option<(i32, usize)> {
    match chars.get(at)? {
        'Z' | 'z' => Some((0, at + 1)),
        sign @ ('+' | '-') => {
            let hours = digits(chars, at + 1, 2)? as i32;
            let (minutes, end) = if chars.get(at + 3) == Some(&':') {
                (digits(chars, at + 4, 2)? as i32, at + 6)
            } else {
                (digits(chars, at + 3, 2)? as i32, at + 5)
            };
            let total = hours * 60 + minutes;
            Some((if *sign == '-' { -total } else { total }, end))
        }
        _ => None,
    }
}

/// Removes a trailing "am", "on", "le", … from `out`.
fn drop_preposition(out: &mut String, language: &str) {
    let prepositions: &[&str] = match language {
        "de" => &["am", "um"],
        "en" => &["on", "at"],
        "fr" => &["le", "à"],
        "es" => &["el"],
        "it" => &["il"],
        "pt" => &["em", "no", "na"],
        _ => &[],
    };
    let trimmed = out.trim_end();
    let start = trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let word = &trimmed[start..];
    if prepositions.iter().any(|p| word.to_lowercase() == *p) {
        *out = trimmed[..start].to_owned();
    }
}
