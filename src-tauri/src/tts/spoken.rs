//! What the speech model is given (spec 066 FR-022b): numbers, clock times, dates, amounts, units,
//! ranges, ordinals and common abbreviations written out in the words of the voice's language.
//!
//! Pocket TTS has no text front end: it reads characters, and digits, "14:30" or "12,90 €" come
//! out garbled. A complete TTS system writes such text out before its model sees it; this is that
//! step for haex. It runs on every sentence any part of haex speaks, so it does not depend on the
//! chat model writing numbers as words. The number grammar lives in `assistant::number_words`;
//! `assistant::speakable` also turns ISO dates into words relative to today before a sentence gets
//! here.
//!
//! What it does not know stays as written: a token it cannot read safely (a version "1.2.3", an IP
//! address, "4K") is passed on unchanged.
//!
//! Dates, times and ordinals are read in `dates`, units and amounts in `units`, the words of each
//! language and the abbreviations are in `words`.

mod dates;
mod units;
mod words;

use crate::assistant::number_words::{cardinal, year_words};
use words::{expand_abbreviations, words, Words};

const LANGUAGES: [&str; 6] = ["de", "en", "fr", "es", "it", "pt"];

/// `text` with numbers and abbreviations as words of `language` (one of de, en, fr, es, it, pt;
/// anything else is read as English).
pub fn spoken(text: &str, language: &str) -> String {
    let language = if LANGUAGES.contains(&language) {
        language
    } else {
        "en"
    };
    let numbers = Reader::new(text, language).read();
    expand_abbreviations(&numbers, language)
}

/// A parsed number: the digits before and after the decimal separator.
#[derive(Debug, Clone, PartialEq)]
struct Number {
    int: String,
    frac: Option<String>,
    /// Written with thousands separators ("1.234").
    grouped: bool,
}

struct Reader<'a> {
    chars: Vec<char>,
    language: &'a str,
    words: Words,
    out: String,
}

impl<'a> Reader<'a> {
    fn new(text: &str, language: &'a str) -> Self {
        Self {
            chars: text.chars().collect(),
            language,
            words: words(language),
            out: String::with_capacity(text.len() + text.len() / 2),
        }
    }

    fn read(mut self) -> String {
        let mut i = 0;
        while i < self.chars.len() {
            match self.at(i) {
                Some(end) => i = end,
                None => {
                    self.out.push(self.chars[i]);
                    i += 1;
                }
            }
        }
        self.out
    }

    fn char(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn is_digit(&self, i: usize) -> bool {
        self.char(i).is_some_and(|c| c.is_ascii_digit())
    }

    /// Whether a token may start at `i`: not in the middle of a word or of "1.2".
    fn starts_token(&self, i: usize) -> bool {
        if i == 0 {
            return true;
        }
        let prev = self.chars[i - 1];
        if prev.is_alphanumeric() || prev == '_' {
            return false;
        }
        if matches!(prev, '.' | ',' | ':' | '/') && i >= 2 && self.chars[i - 2].is_alphanumeric() {
            return false;
        }
        true
    }

    /// Whether the token that ended before `i` stands alone: no letter or digit follows directly.
    fn ends_token(&self, i: usize) -> bool {
        self.char(i)
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    }

    /// Reads whatever starts at `i` and writes its words; returns where reading goes on, or `None`
    /// when nothing starts here.
    fn at(&mut self, i: usize) -> Option<usize> {
        let c = self.chars[i];
        if c.is_ascii_digit() && self.starts_token(i) {
            return Some(self.number_at(i));
        }
        if matches!(c, '-' | '−')
            && self.is_digit(i + 1)
            && (i == 0 || self.chars[i - 1].is_whitespace() || self.chars[i - 1] == '(')
        {
            self.out.push_str(self.words.minus);
            self.out.push(' ');
            return Some(self.number_at(i + 1));
        }
        if c == '+' && self.is_digit(i + 1) && self.starts_token(i) {
            let end = self.digit_run(i + 1);
            self.out.push_str(self.words.plus);
            self.out.push(' ');
            let digits: String = self.chars[i + 1..end].iter().collect();
            self.out.push_str(&self.digit_by_digit(&digits));
            return Some(end);
        }
        // A currency sign before the amount: "€12", "$ 5".
        if self.starts_token(i) {
            if let Some((unit, after)) = self.unit_symbol_at(i, true) {
                let start = if self.char(after) == Some(' ') {
                    after + 1
                } else {
                    after
                };
                if self.is_digit(start) {
                    if let Some((number, end)) = self.parse_number(start) {
                        if self.ends_token(end) {
                            let phrase = self.amount(&number, unit);
                            self.out.push_str(&phrase);
                            return Some(end);
                        }
                    }
                }
            }
        }
        None
    }

    fn digit_run(&self, from: usize) -> usize {
        let mut end = from;
        while self.is_digit(end) {
            end += 1;
        }
        end
    }

    /// Reads the number at `i` (a digit where a token may start) and what belongs to it.
    fn number_at(&mut self, i: usize) -> usize {
        if let Some(end) = self.date_at(i) {
            return end;
        }
        if let Some(end) = self.time_at(i) {
            return self.maybe_range(end);
        }
        let Some((number, end)) = self.parse_number(i) else {
            // "1.2.3", "192.168.0.1": not a number haex can say; keep it whole.
            let mut end = i;
            while self
                .char(end)
                .is_some_and(|c| c.is_ascii_digit() || c == '.')
                && !(self.char(end) == Some('.') && !self.is_digit(end + 1))
            {
                end += 1;
            }
            self.out.extend(&self.chars[i..end]);
            return end;
        };
        if let Some(after) = self.ordinal_at(&number, end) {
            return after;
        }
        if !self.ends_token(end) {
            // Attached units: "10km", "5GB".
            if let Some((unit, after)) = self.unit_symbol_at(end, false) {
                if self.ends_token(after) {
                    let phrase = self.with_unit(&number, unit);
                    self.out.push_str(&phrase);
                    return after;
                }
            }
            // "4K", "3D", "mp3": a name, not a number.
            self.out.extend(&self.chars[i..end]);
            return end;
        }
        // A unit after a space (or a no-break space).
        let gap = if matches!(self.char(end), Some(' ' | '\u{a0}' | '\u{202f}')) {
            end + 1
        } else {
            end
        };
        if let Some((unit, after)) = self.unit_symbol_at(gap, false) {
            if self.unit_ends(after) {
                let phrase = self.with_unit(&number, unit);
                self.out.push_str(&phrase);
                return self.maybe_range(after);
            }
        }
        if self.char(gap) == Some('%') {
            let phrase = format!("{} {}", self.number_words(&number), self.words.percent);
            self.out.push_str(&phrase);
            return gap + 1;
        }
        let phrase = self.plain(&number, end);
        self.out.push_str(&phrase);
        if phrase.chars().any(char::is_alphabetic) && number.int.len() < 6 {
            self.maybe_range(end)
        } else {
            end
        }
    }

    /// Whether a unit symbol that ended before `i` is a whole word ("5 m" but not "5 mal").
    fn unit_ends(&self, i: usize) -> bool {
        self.char(i)
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    }

    /// "10–12", "10 - 12": the word for "to" when another number follows a dash.
    fn maybe_range(&mut self, end: usize) -> usize {
        let mut j = end;
        while self.char(j) == Some(' ') {
            j += 1;
        }
        if !matches!(self.char(j), Some('-' | '–' | '—')) {
            return end;
        }
        let mut k = j + 1;
        while self.char(k) == Some(' ') {
            k += 1;
        }
        if !self.is_digit(k) {
            return end;
        }
        self.out.push(' ');
        self.out.push_str(self.words.to);
        self.out.push(' ');
        k
    }

    fn followed_by(&self, i: usize, text: &str) -> bool {
        (i..)
            .zip(text.chars())
            .all(|(k, c)| self.char(k) == Some(c))
    }

    /// Digits with thousands separators and a decimal part, at `i`: `None` for something that is
    /// no number ("1.2.3").
    fn parse_number(&self, i: usize) -> Option<(Number, usize)> {
        let mut groups = vec![String::new()];
        let mut seps: Vec<char> = Vec::new();
        let mut j = i;
        loop {
            match self.char(j) {
                Some(c) if c.is_ascii_digit() => {
                    groups.last_mut()?.push(c);
                    j += 1;
                }
                Some(c @ ('.' | ',' | '\u{a0}' | '\u{202f}')) if self.is_digit(j + 1) => {
                    seps.push(c);
                    groups.push(String::new());
                    j += 1;
                }
                _ => break,
            }
        }
        let (decimal, thousands) = if self.language == "en" {
            ('.', ',')
        } else {
            (',', '.')
        };
        let is_thousands = |c: char| c == thousands || c == '\u{a0}' || c == '\u{202f}';
        let grouped_ok = |groups: &[String]| {
            (1..=3).contains(&groups[0].len()) && groups[1..].iter().all(|g| g.len() == 3)
        };
        let number = match seps.as_slice() {
            [] => Number {
                int: groups.remove(0),
                frac: None,
                grouped: false,
            },
            all if all.iter().all(|c| *c == all[0]) => {
                if is_thousands(all[0]) && grouped_ok(&groups) {
                    Number {
                        int: groups.concat(),
                        frac: None,
                        grouped: true,
                    }
                } else if all.len() == 1 && (all[0] == decimal || all[0] == thousands) {
                    // One separator that cannot group thousands: "3.5" in German text is a
                    // decimal too.
                    Number {
                        int: groups[0].clone(),
                        frac: Some(groups[1].clone()),
                        grouped: false,
                    }
                } else {
                    return None;
                }
            }
            all => {
                let (last, rest) = all.split_last()?;
                let head = &groups[..groups.len() - 1];
                if rest.iter().all(|c| *c == rest[0] && c != last)
                    && is_thousands(rest[0])
                    && grouped_ok(head)
                {
                    Number {
                        int: head.concat(),
                        frac: groups.last().cloned(),
                        grouped: true,
                    }
                } else {
                    return None;
                }
            }
        };
        Some((number, j))
    }

    /// The last word written so far.
    fn previous_word(&self) -> Option<&str> {
        self.out.split_whitespace().last()
    }

    /// A number standing alone: a year, a long run of digits, or a count.
    fn plain(&self, number: &Number, end: usize) -> String {
        if number.frac.is_none() && !number.grouped {
            let int = &number.int;
            // "Uhr" after a bare hour: "14 Uhr".
            if self.language == "de" && self.followed_by(end, " Uhr") {
                if let Ok(hour @ 0..=24) = int.parse::<u32>() {
                    let h = if hour == 1 {
                        "ein".to_owned()
                    } else {
                        cardinal("de", hour)
                    };
                    return h;
                }
            }
            if self.language == "en" {
                // "5 pm" becomes "five pm"; the model reads "pm" well enough.
                if self.meridiem(end).is_some() {
                    if let Ok(hour @ 1..=12) = int.parse::<u32>() {
                        return cardinal("en", hour);
                    }
                }
            }
            if int.len() == 4 {
                if let Ok(year @ 1100..=2099) = int.parse::<i32>() {
                    return year_words(self.language, year);
                }
            }
            // Telephone numbers, IDs and anything with a leading zero: digit by digit.
            if int.len() >= 6 || (int.len() > 1 && int.starts_with('0')) {
                return self.digit_by_digit(int);
            }
        }
        self.number_words(number)
    }

    /// A count with its decimal part: "drei Komma eins vier".
    fn number_words(&self, number: &Number) -> String {
        let int = match number.int.parse::<u32>() {
            Ok(n) => cardinal(self.language, n),
            Err(_) => self.digit_by_digit(&number.int),
        };
        match &number.frac {
            Some(frac) => format!("{int} {} {}", self.words.decimal, self.digit_by_digit(frac)),
            None => int,
        }
    }

    fn digit_by_digit(&self, digits: &str) -> String {
        digits
            .chars()
            .filter_map(|c| c.to_digit(10))
            .map(|d| cardinal(self.language, d))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
