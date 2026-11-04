//! Dates, clock times and ordinals: "04.11.2026", "14:30 Uhr", "2:30 pm", "am 3. Oktober" (FR-022b).

use super::{Number, Reader};
use crate::assistant::calendar_words::{month_name, time_phrase};
use crate::assistant::number_words::{cardinal, ordinal_en, ordinal_stem_de, year_words};

/// German words after which an ordinal takes "-en": "am dritten", "seit dem zweiten".
const DATIVE_DE: &[&str] = &[
    "am", "vom", "zum", "zur", "im", "beim", "dem", "den", "des", "seit",
];
/// German words after which an ordinal takes "-e": "der zweite Platz".
const NOMINATIVE_DE: &[&str] = &["der", "die", "das", "jeder", "jede", "jedes"];
/// English words after which four digits are a year.
const YEAR_BEFORE_EN: &[&str] = &[
    "in", "since", "until", "till", "by", "from", "year", "of", "before", "after", "around",
    "circa", "to", "through", "during", "early", "late", "mid", "summer", "winter", "spring",
    "fall", "autumn",
];

impl Reader<'_> {
    /// Two digits at `i` as a number.
    pub(super) fn two_digits(&self, i: usize) -> Option<u32> {
        if self.is_digit(i) && self.is_digit(i + 1) {
            Some(self.chars[i].to_digit(10)? * 10 + self.chars[i + 1].to_digit(10)?)
        } else {
            None
        }
    }

    /// One or two digits at `i`: the number and the index after it.
    pub(super) fn short_number(&self, i: usize) -> Option<(u32, usize)> {
        if let Some(n) = self.two_digits(i) {
            if !self.is_digit(i + 2) {
                return Some((n, i + 2));
            }
            return None;
        }
        if self.is_digit(i) && !self.is_digit(i + 1) {
            return Some((self.chars[i].to_digit(10)?, i + 1));
        }
        None
    }

    /// ISO dates ("2026-11-04") and German dates ("04.11.2026", "4.11.").
    pub(super) fn date_at(&mut self, i: usize) -> Option<usize> {
        // ISO: four digits, dash, two, dash, two.
        if (0..4).all(|k| self.is_digit(i + k))
            && self.char(i + 4) == Some('-')
            && self.char(i + 7) == Some('-')
        {
            let year: String = self.chars[i..i + 4].iter().collect();
            let month = self.two_digits(i + 5)?;
            let day = self.two_digits(i + 8)?;
            let end = i + 10;
            if self.is_digit(end) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                return None;
            }
            let phrase = self.date_words(day, month, year.parse().ok());
            self.out.push_str(&phrase);
            return Some(end);
        }
        if self.language != "de" {
            return None;
        }
        let (day, after_day) = self.short_number(i)?;
        if self.char(after_day) != Some('.') {
            return None;
        }
        let (month, after_month) = self.short_number(after_day + 1)?;
        if self.char(after_month) != Some('.')
            || !(1..=31).contains(&day)
            || !(1..=12).contains(&month)
        {
            return None;
        }
        let year_start = after_month + 1;
        let year_end = self.digit_run(year_start);
        let (year, end) = match year_end - year_start {
            4 => (
                self.chars[year_start..year_end]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .ok(),
                year_end,
            ),
            // "4.11." without a year; the period ends the date, not the sentence.
            0 if self.ends_token(year_start) => (None, year_start),
            _ => return None,
        };
        let phrase = self.date_words(day, month, year);
        self.out.push_str(&phrase);
        Some(end)
    }

    pub(super) fn date_words(&self, day: u32, month: u32, year: Option<i32>) -> String {
        let name = month_name(self.language, month);
        let year = year.map(|y| year_words(self.language, y));
        let with_year = |base: String, joiner: &str| match &year {
            Some(y) => format!("{base}{joiner}{y}"),
            None => base,
        };
        match self.language {
            "de" => {
                // "der vierte November", otherwise "am/vom/bis zum vierten November".
                let after_der = self
                    .previous_word()
                    .is_some_and(|w| w.eq_ignore_ascii_case("der"));
                let ending = if after_der { "e" } else { "en" };
                with_year(format!("{}{ending} {name}", ordinal_stem_de(day)), " ")
            }
            "fr" => {
                let d = if day == 1 {
                    "premier".to_owned()
                } else {
                    cardinal("fr", day)
                };
                with_year(format!("{d} {name}"), " ")
            }
            "es" => with_year(format!("{} de {name}", cardinal("es", day)), " de "),
            "it" => {
                let d = if day == 1 {
                    "primo".to_owned()
                } else {
                    cardinal("it", day)
                };
                with_year(format!("{d} {name}"), " ")
            }
            "pt" => {
                let d = if day == 1 {
                    "primeiro".to_owned()
                } else {
                    cardinal("pt", day)
                };
                with_year(format!("{d} de {name}"), " de ")
            }
            _ => with_year(format!("{name} {}", ordinal_en(day)), ", "),
        }
    }

    /// Clock times: "14:30", "14:30 Uhr", "14.30 Uhr", "2:30 pm", "14h30".
    pub(super) fn time_at(&mut self, i: usize) -> Option<usize> {
        let (hour, after_hour) = self.short_number(i)?;
        let sep = self.char(after_hour)?;
        let (minute, mut end) = match sep {
            ':' => {
                let minute = self.two_digits(after_hour + 1)?;
                (minute, after_hour + 3)
            }
            '.' if self.language == "de" => {
                let minute = self.two_digits(after_hour + 1)?;
                if !self.followed_by(after_hour + 3, " Uhr") {
                    return None;
                }
                (minute, after_hour + 3)
            }
            'h' if self.language == "fr" => match self.two_digits(after_hour + 1) {
                Some(minute) => (minute, after_hour + 3),
                None => (0, after_hour + 1),
            },
            _ => return None,
        };
        // "24:00 Uhr" is said in German; elsewhere it is midnight written another way.
        let midnight = hour == 24 && minute == 0 && self.language == "de";
        if (hour > 23 && !midnight) || minute > 59 || self.is_digit(end) {
            return None;
        }
        // Seconds are not said.
        if self.char(end) == Some(':') && self.two_digits(end + 1).is_some() {
            end += 3;
        }
        if !self.ends_token(end) || self.char(end) == Some(':') {
            return None;
        }
        let mut hour = hour;
        match self.language {
            "de" => {
                if self.followed_by(end, " Uhr") {
                    end += " Uhr".len();
                }
            }
            "en" => {
                if let Some((pm, after)) = self.meridiem(end) {
                    // "13:30 pm" is already afternoon.
                    if (1..=12).contains(&hour) {
                        hour = to_24(hour, pm);
                    }
                    end = after;
                }
            }
            _ => {}
        }
        let phrase = time_phrase(self.language, hour, minute, false);
        self.out.push_str(&phrase);
        Some(end)
    }

    /// " am", " pm", "a.m.", "p.m." at `i`: whether it is afternoon, and the index after it.
    pub(super) fn meridiem(&self, i: usize) -> Option<(bool, usize)> {
        let start = if self.char(i) == Some(' ') { i + 1 } else { i };
        for (written, pm) in [
            ("a.m.", false),
            ("p.m.", true),
            ("am", false),
            ("pm", true),
            ("AM", false),
            ("PM", true),
        ] {
            if self.followed_by(start, written) && self.ends_token(start + written.len()) {
                return Some((pm, start + written.len()));
            }
        }
        None
    }

    /// "3." (German), "3rd", "1er", "2º" when the number at hand is an ordinal.
    pub(super) fn ordinal_at(&mut self, number: &Number, end: usize) -> Option<usize> {
        if number.frac.is_some() || number.grouped {
            return None;
        }
        let n: u32 = number.int.parse().ok()?;
        match self.language {
            "de" => self.ordinal_de_at(n, end),
            "en" => {
                for suffix in ["st", "nd", "rd", "th"] {
                    if self.followed_by(end, suffix) && self.ends_token(end + 2) {
                        self.out.push_str(&ordinal_en(n));
                        return Some(end + 2);
                    }
                }
                None
            }
            "fr" => {
                for suffix in ["ème", "eme", "er", "re", "e"] {
                    let len = suffix.chars().count();
                    if self.followed_by(end, suffix) && self.ends_token(end + len) {
                        let words = match (n, suffix) {
                            (1, "re") => "première".to_owned(),
                            (1, _) => "premier".to_owned(),
                            _ => ordinal_fr(n),
                        };
                        self.out.push_str(&words);
                        return Some(end + len);
                    }
                }
                None
            }
            "es" | "it" | "pt" => {
                let after = if self.char(end) == Some('.') {
                    end + 1
                } else {
                    end
                };
                let feminine = match self.char(after) {
                    Some('º' | '°') => false,
                    Some('ª') => true,
                    _ => return None,
                };
                let words = ordinal_romance(self.language, n, feminine)?;
                self.out.push_str(&words);
                Some(after + 1)
            }
            _ => None,
        }
    }

    /// "am 3. Oktober", "der 2. Platz", "3. geworden": a period, a space and a word, where the
    /// period does not end the sentence. "Das sind 42. Danach …" stays a count.
    fn ordinal_de_at(&mut self, n: u32, end: usize) -> Option<usize> {
        if self.char(end) != Some('.') || self.char(end + 1) != Some(' ') {
            return None;
        }
        let next: String = self.chars[end + 2..]
            .iter()
            .take_while(|c| c.is_alphabetic())
            .collect();
        let first = next.chars().next()?;
        let previous = self.previous_word().map(str::to_lowercase);
        let previous = previous.as_deref().unwrap_or("");
        let dative = DATIVE_DE.contains(&previous);
        let is_ordinal = dative
            || NOMINATIVE_DE.contains(&previous)
            || first.is_lowercase()
            || (1..=12).any(|m| month_name("de", m) == next);
        if !is_ordinal {
            return None;
        }
        let ending = if dative { "en" } else { "e" };
        self.out
            .push_str(&format!("{}{ending}", ordinal_stem_de(n)));
        Some(end + 1)
    }

    /// Whether the four digits that ended before `end` are said as a year. English says counts
    /// and years differently ("twelve thirty-four files"), so there a year needs a word that
    /// announces it ("in", "since", a month), the start of a sentence, or no word after it; the
    /// other languages say both alike.
    pub(super) fn reads_as_year(&self, end: usize) -> bool {
        if self.language != "en" {
            return true;
        }
        let previous = self.previous_word().map(str::to_lowercase);
        let announced = match previous.as_deref() {
            None => true,
            Some(w) if w.ends_with(['.', '!', '?', ':']) => true,
            Some(w) => {
                let w = w.trim_matches(|c: char| !c.is_alphanumeric());
                YEAR_BEFORE_EN.contains(&w)
                    || (1..=12).any(|m| month_name("en", m).eq_ignore_ascii_case(w))
            }
        };
        let gap = usize::from(self.char(end) == Some(' '));
        announced || !self.char(end + gap).is_some_and(char::is_alphabetic)
    }
}

fn to_24(hour: u32, pm: bool) -> u32 {
    match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    }
}

/// French ordinals beyond the first: "deuxième", "quatrième", "cinquième", "neuvième".
fn ordinal_fr(n: u32) -> String {
    let words = cardinal("fr", n);
    let stem = if let Some(stem) = words.strip_suffix("cinq") {
        format!("{stem}cinqu")
    } else if let Some(stem) = words.strip_suffix("neuf") {
        format!("{stem}neuv")
    } else if let Some(stem) = words.strip_suffix('e') {
        stem.to_owned()
    } else if let Some(stem) = words.strip_suffix("cents") {
        format!("{stem}cent")
    } else if let Some(stem) = words.strip_suffix("vingts") {
        format!("{stem}vingt")
    } else {
        words
    };
    format!("{stem}ième")
}

/// Spanish, Italian and Portuguese ordinals 1–10 ("1º", "2ª"); larger ones are said as counts.
fn ordinal_romance(language: &str, n: u32, feminine: bool) -> Option<String> {
    let masculine: [&str; 10] = match language {
        "es" => [
            "primero", "segundo", "tercero", "cuarto", "quinto", "sexto", "séptimo", "octavo",
            "noveno", "décimo",
        ],
        "it" => [
            "primo", "secondo", "terzo", "quarto", "quinto", "sesto", "settimo", "ottavo", "nono",
            "decimo",
        ],
        _ => [
            "primeiro", "segundo", "terceiro", "quarto", "quinto", "sexto", "sétimo", "oitavo",
            "nono", "décimo",
        ],
    };
    let word = match n {
        1..=10 => masculine[n as usize - 1],
        _ => return Some(cardinal(language, n)),
    };
    if !feminine {
        return Some(word.to_owned());
    }
    let stem = word.strip_suffix('o')?;
    Some(format!("{stem}a"))
}
