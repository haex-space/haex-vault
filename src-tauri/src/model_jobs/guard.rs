//! The guard over a model's output (spec 055 FR-010, research R10): what is obviously broken is
//! dropped, what fits the format is parsed. It promises no correctness: invented but well-formed
//! content passes (the measurement of 2026-10 showed English sentences and formulas invented on an
//! unreadable photo; nothing here can tell).
//!
//! Order: empty, loop, script switch, JSON, schema. The first violation is the reason.

use std::collections::HashSet;

use serde_json::Value;

use super::schema;

/// From this many words on, fewer than one distinct word in [`LOOP_DISTINCT_DIVISOR`] is a loop.
/// Ordinary German text keeps about one in three; the threshold is far below that on purpose, so
/// a long page of similar lines passes.
const LOOP_MIN_WORDS: usize = 100;
const LOOP_DISTINCT_DIVISOR: usize = 10;
/// The same run of `n` words repeated back to back: at least [`LOOP_MIN_REPEATS`] times and at
/// least [`LOOP_MIN_RUN_WORDS`] words in all. One word on its own needs [`LOOP_SINGLE_WORD_REPEATS`]
/// repeats, so a table full of "ja" cells is no loop.
const LOOP_MIN_REPEATS: usize = 6;
const LOOP_MIN_RUN_WORDS: usize = 12;
const LOOP_SINGLE_WORD_REPEATS: usize = 30;
const LOOP_LONGEST_RUN: usize = 8;
/// The same line (of at least this many characters) three times in a row is a loop.
const LOOP_LINE_MIN_CHARS: usize = 8;
/// Foreign letters make a script switch from this share on (and from this many letters on), so a
/// Greek symbol in a physics note is not one.
const SWITCH_MIN_LETTERS: usize = 3;
const SWITCH_SHARE_DIVISOR: usize = 5;

/// Why an output was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unusable {
    Empty,
    Loop,
    ScriptSwitch,
    InvalidJson,
    /// The output is JSON but violates the format; the text names the first violation.
    Schema(String),
}

impl Unusable {
    /// A short reason for the provenance and the error message.
    pub fn reason(&self) -> String {
        match self {
            Unusable::Empty => "empty".into(),
            Unusable::Loop => "loop".into(),
            Unusable::ScriptSwitch => "script switch".into(),
            Unusable::InvalidJson => "invalid json".into(),
            Unusable::Schema(violation) => format!("schema: {violation}"),
        }
    }
}

/// Checks an output. `context` is the text the model was given (instruction and input text): the
/// scripts in it are the scripts the answer may use.
pub fn check(output: &str, context: &str, schema: &Value) -> Result<Value, Unusable> {
    let body = strip_fence(output.trim());
    if body.is_empty() {
        return Err(Unusable::Empty);
    }
    let parsed = serde_json::from_str::<Value>(body).ok();
    // Judge the text the answer is made of: with valid JSON its strings, otherwise everything.
    let text = match &parsed {
        Some(value) => {
            let mut strings = Vec::new();
            collect_strings(value, &mut strings);
            strings.join("\n")
        }
        None => body.to_string(),
    };
    if is_loop(&text) {
        return Err(Unusable::Loop);
    }
    if is_script_switch(&text, context) {
        return Err(Unusable::ScriptSwitch);
    }
    let value = parsed.ok_or(Unusable::InvalidJson)?;
    schema::validate(schema, &value).map_err(Unusable::Schema)?;
    Ok(value)
}

/// A reply in a code fence (```` ```json ````) is the JSON inside it.
fn strip_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let rest = rest
        .split_once('\n')
        .map_or("", |(_, after)| after)
        .trim_end();
    rest.strip_suffix("```").unwrap_or(rest).trim()
}

fn collect_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| collect_strings(item, out)),
        _ => {}
    }
}

fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_loop(text: &str) -> bool {
    let words = words(text);
    if words.len() >= LOOP_MIN_WORDS {
        let distinct: HashSet<&str> = words.iter().copied().collect();
        if distinct.len() * LOOP_DISTINCT_DIVISOR < words.len() {
            return true;
        }
    }
    has_repeated_run(&words) || has_repeated_line(text)
}

/// Whether some run of 1 to [`LOOP_LONGEST_RUN`] words repeats back to back often enough (a table
/// that alternates "ja nein" twelve times is caught too, a known and accepted price). A run of
/// `n` words repeated `r` times shows as `n * (r - 1)` positions that equal the word `n` places
/// before them.
fn has_repeated_run(words: &[&str]) -> bool {
    for n in 1..=LOOP_LONGEST_RUN {
        let needed = if n == 1 {
            LOOP_SINGLE_WORD_REPEATS
        } else {
            LOOP_MIN_REPEATS.max(LOOP_MIN_RUN_WORDS.div_ceil(n))
        };
        let mut matching = 0;
        for position in n..words.len() {
            // A unit of one word repeated is the single-word case, which has its own, higher
            // threshold; without this, 20 "ja" cells would count as ten repeats of "ja ja".
            let unit = &words[position - n..position];
            let one_word = unit.iter().all(|word| *word == unit[0]);
            if words[position] == words[position - n] && (n == 1 || !one_word) {
                matching += 1;
                if matching >= n * (needed - 1) {
                    return true;
                }
            } else {
                matching = 0;
            }
        }
    }
    false
}

fn has_repeated_line(text: &str) -> bool {
    let mut previous: Option<&str> = None;
    let mut run = 1;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.chars().count() >= LOOP_LINE_MIN_CHARS && previous == Some(line) {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 1;
        }
        previous = Some(line);
    }
    false
}

/// A script that matters here. Latin and everything not listed (digits, punctuation, symbols) never
/// counts as foreign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Script {
    Latin,
    Greek,
    Cyrillic,
    Armenian,
    Hebrew,
    Arabic,
    Devanagari,
    Bengali,
    Tamil,
    Thai,
    Georgian,
    Hangul,
    Kana,
    Han,
}

fn script_of(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    Some(match c as u32 {
        0x41..=0x5A | 0x61..=0x7A | 0xAA | 0xBA | 0xC0..=0x24F | 0x1E00..=0x1EFF => Script::Latin,
        0x370..=0x3FF | 0x1F00..=0x1FFF => Script::Greek,
        0x400..=0x52F => Script::Cyrillic,
        0x530..=0x58F => Script::Armenian,
        0x590..=0x5FF => Script::Hebrew,
        0x600..=0x6FF | 0x750..=0x77F => Script::Arabic,
        0x900..=0x97F => Script::Devanagari,
        0x980..=0x9FF => Script::Bengali,
        0xB80..=0xBFF => Script::Tamil,
        0xE00..=0xE7F => Script::Thai,
        0x10A0..=0x10FF => Script::Georgian,
        0x1100..=0x11FF | 0xAC00..=0xD7AF => Script::Hangul,
        0x3040..=0x30FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF => Script::Han,
        _ => return None,
    })
}

/// Whether the answer is, to a good part, in a script that neither Latin nor the context uses.
fn is_script_switch(text: &str, context: &str) -> bool {
    let known: HashSet<Script> = context.chars().filter_map(script_of).collect();
    let mut letters = 0usize;
    let mut foreign = 0usize;
    for script in text.chars().filter_map(script_of) {
        letters += 1;
        if script != Script::Latin && !known.contains(&script) {
            foreign += 1;
        }
    }
    foreign >= SWITCH_MIN_LETTERS && foreign * SWITCH_SHARE_DIVISOR >= letters
}
