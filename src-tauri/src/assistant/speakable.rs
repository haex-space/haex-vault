//! Turns the model's answer into what haex says (spec 066 FR-022, research R10): no formatting
//! marks; a table, a code block or a written-out link becomes a short pointer to the chat; ISO
//! dates and times are spoken as words relative to today ("morgen um zehn Uhr"); an answer longer
//! than four sentences is cut after three, with "Soll ich mehr vorlesen?" (FR-021, T057). The chat
//! and the transcript keep the full text.
//!
//! Date and time words are grammar, so they are code with one branch per spoken language; the
//! sentences come from the word list.

use super::lexicon::{self, FALLBACK_LANGUAGE};
use super::sentences::{fence_closer, fence_opener, Fence, Sentences};

/// Sentences an answer may have before it is cut.
pub const MAX_SENTENCES: usize = 4;
/// Sentences spoken before "Soll ich mehr vorlesen?" when it is cut.
pub const SUMMARY_SENTENCES: usize = 3;

/// A local calendar date and wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Civil {
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    /// Days since 1970-01-01 of the date (H. Hinnant's `days_from_civil`).
    fn days(&self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn from_days(days: i64, hour: u32, minute: u32) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self::new(year, month, day, hour, minute)
    }

    fn minutes(&self) -> i64 {
        self.days() * 1440 + i64::from(self.hour) * 60 + i64::from(self.minute)
    }

    fn from_minutes(minutes: i64) -> Self {
        let days = minutes.div_euclid(1440);
        let rest = minutes.rem_euclid(1440);
        Self::from_days(days, (rest / 60) as u32, (rest % 60) as u32)
    }

    /// Monday = 0.
    fn weekday(&self) -> usize {
        (self.days() + 3).rem_euclid(7) as usize
    }

    fn valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= 31
            && self.hour < 24
            && self.minute < 60
            && Self::from_days(self.days(), self.hour, self.minute) == *self
    }
}

/// "Now" as the device sees it: local time and the offset of local time from UTC, so a time with
/// an offset ("…Z", "…+02:00") is spoken in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub now: Civil,
    pub utc_offset_minutes: i32,
}

/// The sentences of a whole answer: what is spoken, and what "mehr vorlesen" would read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Spoken {
    pub sentences: Vec<String>,
    pub rest: Vec<String>,
}

/// [`Speech`] over a complete text.
pub fn speak_all(text: &str, language: &str, clock: Clock) -> Spoken {
    let mut speech = Speech::new(language, clock);
    let mut sentences = speech.push(text);
    sentences.extend(speech.finish());
    Spoken {
        sentences,
        rest: speech.rest().to_vec(),
    }
}

/// The streamed answer as spoken sentences: token deltas in, sentences to say out.
#[derive(Debug)]
pub struct Speech {
    language: String,
    sentences: Sentences,
    lines: Speakable,
    spoken: usize,
    held: Option<String>,
    cut: bool,
    rest: Vec<String>,
}

impl Speech {
    pub fn new(language: &str, clock: Clock) -> Self {
        Self {
            language: language.to_owned(),
            sentences: Sentences::new(),
            lines: Speakable::new(language, clock),
            spoken: 0,
            held: None,
            cut: false,
            rest: Vec::new(),
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
        let mut out = self.admit_all(pieces);
        if let Some(fourth) = self.held.take() {
            out.push(fourth);
        }
        out
    }

    /// The sentences not spoken because the answer was cut, ready to speak.
    pub fn rest(&self) -> &[String] {
        &self.rest
    }

    fn admit_all(&mut self, pieces: Vec<String>) -> Vec<String> {
        let mut out = Vec::new();
        for piece in pieces {
            if let Some(sentence) = self.lines.line(&piece) {
                self.admit(sentence, &mut out);
            }
        }
        out
    }

    /// The first three sentences go out at once; the fourth waits until either the answer ends
    /// (then it is spoken) or a fifth arrives (then the answer is cut).
    fn admit(&mut self, sentence: String, out: &mut Vec<String>) {
        if self.cut {
            self.rest.push(sentence);
        } else if self.spoken < SUMMARY_SENTENCES {
            self.spoken += 1;
            out.push(sentence);
        } else if let Some(fourth) = self.held.take() {
            debug_assert_eq!(SUMMARY_SENTENCES + 1, MAX_SENTENCES);
            self.cut = true;
            self.rest.push(fourth);
            self.rest.push(sentence);
            out.push(lexicon::say("say.askMore", &self.language, &[]));
        } else {
            self.held = Some(sentence);
        }
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

// --- Words for numbers, days and times, one branch per language (T151: native speakers check) ---

const MONTHS: [(&str, [&str; 12]); 6] = [
    (
        "de",
        [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ],
    ),
    (
        "en",
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ],
    ),
    (
        "fr",
        [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ],
    ),
    (
        "es",
        [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ],
    ),
    (
        "it",
        [
            "gennaio",
            "febbraio",
            "marzo",
            "aprile",
            "maggio",
            "giugno",
            "luglio",
            "agosto",
            "settembre",
            "ottobre",
            "novembre",
            "dicembre",
        ],
    ),
    (
        "pt",
        [
            "janeiro",
            "fevereiro",
            "março",
            "abril",
            "maio",
            "junho",
            "julho",
            "agosto",
            "setembro",
            "outubro",
            "novembro",
            "dezembro",
        ],
    ),
];

/// The weekday as it follows the language's "on": "am Montag", "on Monday", "lundi", …
const WEEKDAYS: [(&str, [&str; 7]); 6] = [
    (
        "de",
        [
            "am Montag",
            "am Dienstag",
            "am Mittwoch",
            "am Donnerstag",
            "am Freitag",
            "am Samstag",
            "am Sonntag",
        ],
    ),
    (
        "en",
        [
            "on Monday",
            "on Tuesday",
            "on Wednesday",
            "on Thursday",
            "on Friday",
            "on Saturday",
            "on Sunday",
        ],
    ),
    (
        "fr",
        [
            "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
        ],
    ),
    (
        "es",
        [
            "el lunes",
            "el martes",
            "el miércoles",
            "el jueves",
            "el viernes",
            "el sábado",
            "el domingo",
        ],
    ),
    (
        "it",
        [
            "lunedì",
            "martedì",
            "mercoledì",
            "giovedì",
            "venerdì",
            "sabato",
            "domenica",
        ],
    ),
    (
        "pt",
        [
            "na segunda-feira",
            "na terça-feira",
            "na quarta-feira",
            "na quinta-feira",
            "na sexta-feira",
            "no sábado",
            "no domingo",
        ],
    ),
];

/// Today, tomorrow, yesterday, the day after tomorrow, the day before yesterday.
const RELATIVE_DAYS: [(&str, [&str; 5]); 6] = [
    (
        "de",
        ["heute", "morgen", "gestern", "übermorgen", "vorgestern"],
    ),
    (
        "en",
        [
            "today",
            "tomorrow",
            "yesterday",
            "the day after tomorrow",
            "the day before yesterday",
        ],
    ),
    (
        "fr",
        [
            "aujourd'hui",
            "demain",
            "hier",
            "après-demain",
            "avant-hier",
        ],
    ),
    ("es", ["hoy", "mañana", "ayer", "pasado mañana", "anteayer"]),
    (
        "it",
        ["oggi", "domani", "ieri", "dopodomani", "l'altro ieri"],
    ),
    (
        "pt",
        ["hoje", "amanhã", "ontem", "depois de amanhã", "anteontem"],
    ),
];

fn table<'a, const N: usize>(
    rows: &'a [(&str, [&'a str; N]); 6],
    language: &str,
) -> &'a [&'a str; N] {
    rows.iter()
        .find(|(code, _)| *code == language)
        .map_or(&rows[1].1, |(_, row)| row)
}

/// How a date is said, relative to `today`.
pub fn day_phrase(language: &str, date: Civil, today: Civil) -> String {
    let diff = date.days() - today.days();
    let relative = table(&RELATIVE_DAYS, language);
    match diff {
        0 => return relative[0].to_owned(),
        1 => return relative[1].to_owned(),
        -1 => return relative[2].to_owned(),
        2 => return relative[3].to_owned(),
        -2 => return relative[4].to_owned(),
        3..=6 => return table(&WEEKDAYS, language)[date.weekday()].to_owned(),
        _ => {}
    }
    let month = table(&MONTHS, language)[date.month as usize - 1];
    let d = date.day;
    let with_year = date.year != today.year;
    let year = year_words(language, date.year);
    match language {
        "de" => {
            let base = format!("am {} {month}", ordinal_de(d));
            if with_year {
                format!("{base} {year}")
            } else {
                base
            }
        }
        "fr" => {
            let day = if d == 1 {
                "premier".to_owned()
            } else {
                cardinal(language, d)
            };
            let base = format!("le {day} {month}");
            if with_year {
                format!("{base} {year}")
            } else {
                base
            }
        }
        "es" => {
            let base = format!("el {} de {month}", cardinal(language, d));
            if with_year {
                format!("{base} de {year}")
            } else {
                base
            }
        }
        "it" => {
            let base = match d {
                1 => format!("il primo {month}"),
                8 | 11 => format!("l'{} {month}", cardinal(language, d)),
                _ => format!("il {} {month}", cardinal(language, d)),
            };
            if with_year {
                format!("{base} {year}")
            } else {
                base
            }
        }
        "pt" => {
            let day = if d == 1 {
                "primeiro".to_owned()
            } else {
                cardinal(language, d)
            };
            let base = format!("em {day} de {month}");
            if with_year {
                format!("{base} de {year}")
            } else {
                base
            }
        }
        _ => {
            let base = format!("on {month} {}", ordinal_en(d));
            if with_year {
                format!("{base}, {year}")
            } else {
                base
            }
        }
    }
}

/// How a clock time is said; `with_preposition` adds "um", "at", "à", "a las", … for a time that
/// follows a day.
pub fn time_phrase(language: &str, hour: u32, minute: u32, with_preposition: bool) -> String {
    let minutes = |joiner: &str| {
        if minute == 0 {
            String::new()
        } else {
            format!("{joiner}{}", cardinal(language, minute))
        }
    };
    match language {
        "de" => {
            let h = if hour == 1 {
                "ein".to_owned()
            } else {
                cardinal(language, hour)
            };
            let bare = format!("{h} Uhr{}", minutes(" "));
            if with_preposition {
                format!("um {bare}")
            } else {
                bare
            }
        }
        "fr" => {
            let bare = match (hour, minute) {
                (0, _) => format!("minuit{}", fem_fr(minute, " ")),
                (12, _) => format!("midi{}", fem_fr(minute, " ")),
                (1, _) => format!("une heure{}", fem_fr(minute, " ")),
                _ => format!("{} heures{}", fem_fr(hour, ""), fem_fr(minute, " ")),
            };
            if with_preposition {
                format!("à {bare}")
            } else {
                bare
            }
        }
        "es" => {
            let h = if hour == 1 || hour == 21 {
                cardinal(language, hour).replace("uno", "una")
            } else {
                cardinal(language, hour)
            };
            let bare = format!("{h}{}", minutes(" y "));
            match (with_preposition, hour, minute) {
                (false, _, _) => bare,
                (true, 0, 0) => "a medianoche".to_owned(),
                (true, 1, _) => format!("a la {bare}"),
                (true, _, _) => format!("a las {bare}"),
            }
        }
        "it" => {
            let h = if hour == 1 {
                "una".to_owned()
            } else {
                cardinal(language, hour)
            };
            let bare = format!("{h}{}", minutes(" e "));
            match (with_preposition, hour, minute) {
                (false, _, _) => bare,
                (true, 0, 0) => "a mezzanotte".to_owned(),
                (true, 1, _) => format!("all'{bare}"),
                (true, _, _) => format!("alle {bare}"),
            }
        }
        "pt" => {
            let bare = format!("{}{}", fem_pt(hour), minutes(" e "));
            match (with_preposition, hour, minute) {
                (false, _, _) => bare,
                (true, 0, 0) => "à meia-noite".to_owned(),
                (true, 12, 0) => "ao meio-dia".to_owned(),
                (true, 1, _) => format!("à {bare}"),
                (true, _, _) => format!("às {bare}"),
            }
        }
        _ => {
            let bare = match (hour, minute) {
                (0, 0) => "midnight".to_owned(),
                (12, 0) => "noon".to_owned(),
                _ => {
                    let h12 = if hour.is_multiple_of(12) {
                        12
                    } else {
                        hour % 12
                    };
                    let m = match minute {
                        0 => String::new(),
                        1..=9 => format!(" oh {}", cardinal("en", minute)),
                        _ => format!(" {}", cardinal("en", minute)),
                    };
                    let part = match hour {
                        13..=17 => " in the afternoon",
                        18..=23 => " in the evening",
                        0..=4 => " at night",
                        _ => "",
                    };
                    format!("{}{m}{part}", cardinal("en", h12))
                }
            };
            if with_preposition {
                format!("at {bare}")
            } else {
                bare
            }
        }
    }
}

/// Cardinal number words for 0–99; larger numbers stay digits.
pub fn cardinal(language: &str, n: u32) -> String {
    if n > 99 {
        return n.to_string();
    }
    let n = n as usize;
    match language {
        "de" => {
            const ONES: [&str; 20] = [
                "null",
                "eins",
                "zwei",
                "drei",
                "vier",
                "fünf",
                "sechs",
                "sieben",
                "acht",
                "neun",
                "zehn",
                "elf",
                "zwölf",
                "dreizehn",
                "vierzehn",
                "fünfzehn",
                "sechzehn",
                "siebzehn",
                "achtzehn",
                "neunzehn",
            ];
            const TENS: [&str; 10] = [
                "", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig",
                "achtzig", "neunzig",
            ];
            match (n, n % 10) {
                (0..=19, _) => ONES[n].to_owned(),
                (_, 0) => TENS[n / 10].to_owned(),
                (_, 1) => format!("einund{}", TENS[n / 10]),
                (_, u) => format!("{}und{}", ONES[u], TENS[n / 10]),
            }
        }
        "fr" => {
            const ONES: [&str; 20] = [
                "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf",
                "dix", "onze", "douze", "treize", "quatorze", "quinze", "seize", "dix-sept",
                "dix-huit", "dix-neuf",
            ];
            const TENS: [&str; 7] = [
                "",
                "",
                "vingt",
                "trente",
                "quarante",
                "cinquante",
                "soixante",
            ];
            match n {
                0..=19 => ONES[n].to_owned(),
                20..=69 => match n % 10 {
                    0 => TENS[n / 10].to_owned(),
                    1 => format!("{} et un", TENS[n / 10]),
                    u => format!("{}-{}", TENS[n / 10], ONES[u]),
                },
                71 => "soixante et onze".to_owned(),
                70..=79 => format!("soixante-{}", ONES[n - 60]),
                80 => "quatre-vingts".to_owned(),
                _ => format!("quatre-vingt-{}", ONES[n - 80]),
            }
        }
        "es" => {
            const ONES: [&str; 30] = [
                "cero",
                "uno",
                "dos",
                "tres",
                "cuatro",
                "cinco",
                "seis",
                "siete",
                "ocho",
                "nueve",
                "diez",
                "once",
                "doce",
                "trece",
                "catorce",
                "quince",
                "dieciséis",
                "diecisiete",
                "dieciocho",
                "diecinueve",
                "veinte",
                "veintiuno",
                "veintidós",
                "veintitrés",
                "veinticuatro",
                "veinticinco",
                "veintiséis",
                "veintisiete",
                "veintiocho",
                "veintinueve",
            ];
            const TENS: [&str; 10] = [
                "",
                "",
                "",
                "treinta",
                "cuarenta",
                "cincuenta",
                "sesenta",
                "setenta",
                "ochenta",
                "noventa",
            ];
            match (n, n % 10) {
                (0..=29, _) => ONES[n].to_owned(),
                (_, 0) => TENS[n / 10].to_owned(),
                (_, u) => format!("{} y {}", TENS[n / 10], ONES[u]),
            }
        }
        "it" => {
            const ONES: [&str; 20] = [
                "zero",
                "uno",
                "due",
                "tre",
                "quattro",
                "cinque",
                "sei",
                "sette",
                "otto",
                "nove",
                "dieci",
                "undici",
                "dodici",
                "tredici",
                "quattordici",
                "quindici",
                "sedici",
                "diciassette",
                "diciotto",
                "diciannove",
            ];
            const TENS: [&str; 10] = [
                "",
                "",
                "venti",
                "trenta",
                "quaranta",
                "cinquanta",
                "sessanta",
                "settanta",
                "ottanta",
                "novanta",
            ];
            match (n, n % 10) {
                (0..=19, _) => ONES[n].to_owned(),
                (_, 0) => TENS[n / 10].to_owned(),
                (_, u @ (1 | 8)) => {
                    let tens = TENS[n / 10];
                    format!("{}{}", &tens[..tens.len() - 1], ONES[u])
                }
                (_, 3) => format!("{}tré", TENS[n / 10]),
                (_, u) => format!("{}{}", TENS[n / 10], ONES[u]),
            }
        }
        "pt" => {
            const ONES: [&str; 20] = [
                "zero",
                "um",
                "dois",
                "três",
                "quatro",
                "cinco",
                "seis",
                "sete",
                "oito",
                "nove",
                "dez",
                "onze",
                "doze",
                "treze",
                "catorze",
                "quinze",
                "dezesseis",
                "dezessete",
                "dezoito",
                "dezenove",
            ];
            const TENS: [&str; 10] = [
                "",
                "",
                "vinte",
                "trinta",
                "quarenta",
                "cinquenta",
                "sessenta",
                "setenta",
                "oitenta",
                "noventa",
            ];
            match (n, n % 10) {
                (0..=19, _) => ONES[n].to_owned(),
                (_, 0) => TENS[n / 10].to_owned(),
                (_, u) => format!("{} e {}", TENS[n / 10], ONES[u]),
            }
        }
        _ => {
            const ONES: [&str; 20] = [
                "zero",
                "one",
                "two",
                "three",
                "four",
                "five",
                "six",
                "seven",
                "eight",
                "nine",
                "ten",
                "eleven",
                "twelve",
                "thirteen",
                "fourteen",
                "fifteen",
                "sixteen",
                "seventeen",
                "eighteen",
                "nineteen",
            ];
            const TENS: [&str; 10] = [
                "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty",
                "ninety",
            ];
            match (n, n % 10) {
                (0..=19, _) => ONES[n].to_owned(),
                (_, 0) => TENS[n / 10].to_owned(),
                (_, u) => format!("{}-{}", TENS[n / 10], ONES[u]),
            }
        }
    }
}

/// "am …ten": the German dative ordinal of a day of the month.
fn ordinal_de(d: u32) -> String {
    match d {
        1 => "ersten".to_owned(),
        3 => "dritten".to_owned(),
        7 => "siebten".to_owned(),
        8 => "achten".to_owned(),
        2..=19 => format!("{}ten", cardinal("de", d)),
        _ => format!("{}sten", cardinal("de", d)),
    }
}

fn ordinal_en(d: u32) -> String {
    fn small(n: u32) -> String {
        match n {
            1 => "first".to_owned(),
            2 => "second".to_owned(),
            3 => "third".to_owned(),
            5 => "fifth".to_owned(),
            8 => "eighth".to_owned(),
            9 => "ninth".to_owned(),
            12 => "twelfth".to_owned(),
            _ => format!("{}th", cardinal("en", n)),
        }
    }
    match (d, d % 10) {
        (0..=19, _) => small(d),
        (_, 0) => format!("{}ieth", cardinal("en", d).trim_end_matches('y')),
        (_, u) => format!("{}-{}", cardinal("en", d - u), small(u)),
    }
}

/// A year of this century in words; other years stay digits.
fn year_words(language: &str, year: i32) -> String {
    if !(2000..=2099).contains(&year) {
        return year.to_string();
    }
    let x = (year - 2000) as u32;
    match (language, x) {
        ("de", 0) => "zweitausend".to_owned(),
        ("de", _) => format!("zweitausend{}", cardinal("de", x)),
        ("fr", 0) => "deux mille".to_owned(),
        ("fr", _) => format!("deux mille {}", cardinal("fr", x)),
        ("es", 0) => "dos mil".to_owned(),
        ("es", _) => format!("dos mil {}", cardinal("es", x)),
        ("it", 0) => "duemila".to_owned(),
        ("it", _) => format!("duemila{}", cardinal("it", x)),
        ("pt", 0) => "dois mil".to_owned(),
        ("pt", _) => format!("dois mil e {}", cardinal("pt", x)),
        (_, 0) => "two thousand".to_owned(),
        (_, 1..=9) => format!("two thousand {}", cardinal("en", x)),
        _ => format!("twenty {}", cardinal("en", x)),
    }
}

/// French feminine number (hours and minutes of a time): "une", "vingt et une"; empty for 0
/// minutes. `joiner` goes before it.
fn fem_fr(n: u32, joiner: &str) -> String {
    if n == 0 && !joiner.is_empty() {
        return String::new();
    }
    let words = cardinal("fr", n);
    let words = if words == "un" || words.ends_with(" et un") {
        format!("{words}e")
    } else {
        words
    };
    format!("{joiner}{words}")
}

/// Portuguese feminine number for an hour: "uma", "duas", "vinte e uma".
fn fem_pt(n: u32) -> String {
    let words = cardinal("pt", n);
    if let Some(stem) = words.strip_suffix("um") {
        if stem.is_empty() || stem.ends_with("e ") {
            return format!("{stem}uma");
        }
    }
    if let Some(stem) = words.strip_suffix("dois") {
        if stem.is_empty() || stem.ends_with("e ") {
            return format!("{stem}duas");
        }
    }
    words
}
