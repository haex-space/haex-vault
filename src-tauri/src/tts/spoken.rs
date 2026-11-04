//! What the speech model is given (spec 066 FR-022b): numbers, clock times, dates, amounts, units,
//! ranges, ordinals and common abbreviations written out in the words of the voice's language.
//!
//! Pocket TTS has no text front end: it reads characters, and digits, "14:30" or "12,90 €" come
//! out garbled. A complete TTS system writes such text out before its model sees it; this is that
//! step for haex. It runs on every sentence any part of haex speaks, so it does not depend on the
//! chat model writing numbers as words. The number grammar lives in `assistant::speakable`, which
//! also turns ISO dates into words relative to today before a sentence gets here.
//!
//! What it does not know stays as written: a token it cannot read safely (a version "1.2.3", an IP
//! address, "4K") is passed on unchanged.

use crate::assistant::speakable::{
    cardinal, month_name, ordinal_en, ordinal_stem_de, time_phrase, year_words,
};

/// The words of `language` that numbers need.
struct Words {
    /// The decimal separator as it is spoken ("Komma", "point").
    decimal: &'static str,
    minus: &'static str,
    plus: &'static str,
    /// Between the ends of a range ("10–12": "zehn bis zwölf").
    to: &'static str,
    percent: &'static str,
    /// "eins" is "ein"/"eine" before a noun: masculine/neuter and feminine.
    one: (&'static str, &'static str),
}

fn words(language: &str) -> Words {
    match language {
        "de" => Words {
            decimal: "Komma",
            minus: "minus",
            plus: "plus",
            to: "bis",
            percent: "Prozent",
            one: ("ein", "eine"),
        },
        "fr" => Words {
            decimal: "virgule",
            minus: "moins",
            plus: "plus",
            to: "à",
            percent: "pour cent",
            one: ("un", "une"),
        },
        "es" => Words {
            decimal: "coma",
            minus: "menos",
            plus: "más",
            to: "a",
            percent: "por ciento",
            one: ("un", "una"),
        },
        "it" => Words {
            decimal: "virgola",
            minus: "meno",
            plus: "più",
            to: "a",
            percent: "per cento",
            one: ("un", "una"),
        },
        "pt" => Words {
            decimal: "vírgula",
            minus: "menos",
            plus: "mais",
            to: "a",
            percent: "por cento",
            one: ("um", "uma"),
        },
        _ => Words {
            decimal: "point",
            minus: "minus",
            plus: "plus",
            to: "to",
            percent: "percent",
            one: ("one", "one"),
        },
    }
}

/// A noun after a number, in one language: the word for one, the word for more, and whether it is
/// feminine (for "eine Stunde", "une heure").
type Noun = (&'static str, &'static str, bool);

/// A unit or currency: the ways it is written after (or, for a currency sign, before) a number,
/// and its noun in de, en, fr, es, it, pt.
struct Unit {
    symbols: &'static [&'static str],
    nouns: [Noun; 6],
    /// A currency: "12,90 €" says the cents after the unit.
    currency: bool,
}

const LANGUAGES: [&str; 6] = ["de", "en", "fr", "es", "it", "pt"];

/// Longer symbols first, so "km/h" wins over "km" and "kWh" over "kW".
const UNITS: &[Unit] = &[
    Unit {
        symbols: &["€", "EUR"],
        nouns: [
            ("Euro", "Euro", false),
            ("euro", "euros", false),
            ("euro", "euros", false),
            ("euro", "euros", false),
            ("euro", "euro", false),
            ("euro", "euros", false),
        ],
        currency: true,
    },
    Unit {
        symbols: &["$", "USD"],
        nouns: [
            ("Dollar", "Dollar", false),
            ("dollar", "dollars", false),
            ("dollar", "dollars", false),
            ("dólar", "dólares", false),
            ("dollaro", "dollari", false),
            ("dólar", "dólares", false),
        ],
        currency: true,
    },
    Unit {
        symbols: &["£", "GBP"],
        nouns: [
            ("Pfund", "Pfund", false),
            ("pound", "pounds", false),
            ("livre", "livres", true),
            ("libra", "libras", true),
            ("sterlina", "sterline", true),
            ("libra", "libras", true),
        ],
        currency: true,
    },
    Unit {
        symbols: &["CHF"],
        nouns: [
            ("Franken", "Franken", false),
            ("franc", "francs", false),
            ("franc", "francs", false),
            ("franco", "francos", false),
            ("franco", "franchi", false),
            ("franco", "francos", false),
        ],
        currency: true,
    },
    Unit {
        symbols: &["km/h"],
        nouns: [
            ("Kilometer pro Stunde", "Kilometer pro Stunde", false),
            ("kilometer per hour", "kilometers per hour", false),
            ("kilomètre heure", "kilomètres heure", false),
            ("kilómetro por hora", "kilómetros por hora", false),
            ("chilometro orario", "chilometri orari", false),
            ("quilômetro por hora", "quilômetros por hora", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["kWh"],
        nouns: [
            ("Kilowattstunde", "Kilowattstunden", true),
            ("kilowatt hour", "kilowatt hours", false),
            ("kilowattheure", "kilowattheures", true),
            ("kilovatio hora", "kilovatios hora", false),
            ("chilowattora", "chilowattora", false),
            ("quilowatt-hora", "quilowatts-hora", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["kW"],
        nouns: [
            ("Kilowatt", "Kilowatt", false),
            ("kilowatt", "kilowatts", false),
            ("kilowatt", "kilowatts", false),
            ("kilovatio", "kilovatios", false),
            ("chilowatt", "chilowatt", false),
            ("quilowatt", "quilowatts", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["km"],
        nouns: [
            ("Kilometer", "Kilometer", false),
            ("kilometer", "kilometers", false),
            ("kilomètre", "kilomètres", false),
            ("kilómetro", "kilómetros", false),
            ("chilometro", "chilometri", false),
            ("quilômetro", "quilômetros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["cm"],
        nouns: [
            ("Zentimeter", "Zentimeter", false),
            ("centimeter", "centimeters", false),
            ("centimètre", "centimètres", false),
            ("centímetro", "centímetros", false),
            ("centimetro", "centimetri", false),
            ("centímetro", "centímetros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["mm"],
        nouns: [
            ("Millimeter", "Millimeter", false),
            ("millimeter", "millimeters", false),
            ("millimètre", "millimètres", false),
            ("milímetro", "milímetros", false),
            ("millimetro", "millimetri", false),
            ("milímetro", "milímetros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["kg"],
        nouns: [
            ("Kilogramm", "Kilogramm", false),
            ("kilogram", "kilograms", false),
            ("kilogramme", "kilogrammes", false),
            ("kilogramo", "kilogramos", false),
            ("chilogrammo", "chilogrammi", false),
            ("quilograma", "quilogramas", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["ml"],
        nouns: [
            ("Milliliter", "Milliliter", false),
            ("milliliter", "milliliters", false),
            ("millilitre", "millilitres", false),
            ("mililitro", "mililitros", false),
            ("millilitro", "millilitri", false),
            ("mililitro", "mililitros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["min"],
        nouns: [
            ("Minute", "Minuten", true),
            ("minute", "minutes", false),
            ("minute", "minutes", true),
            ("minuto", "minutos", false),
            ("minuto", "minuti", false),
            ("minuto", "minutos", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["GB"],
        nouns: [
            ("Gigabyte", "Gigabyte", false),
            ("gigabyte", "gigabytes", false),
            ("gigaoctet", "gigaoctets", false),
            ("gigabyte", "gigabytes", false),
            ("gigabyte", "gigabyte", false),
            ("gigabyte", "gigabytes", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["MB"],
        nouns: [
            ("Megabyte", "Megabyte", false),
            ("megabyte", "megabytes", false),
            ("mégaoctet", "mégaoctets", false),
            ("megabyte", "megabytes", false),
            ("megabyte", "megabyte", false),
            ("megabyte", "megabytes", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["TB"],
        nouns: [
            ("Terabyte", "Terabyte", false),
            ("terabyte", "terabytes", false),
            ("téraoctet", "téraoctets", false),
            ("terabyte", "terabytes", false),
            ("terabyte", "terabyte", false),
            ("terabyte", "terabytes", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["KB", "kB"],
        nouns: [
            ("Kilobyte", "Kilobyte", false),
            ("kilobyte", "kilobytes", false),
            ("kilooctet", "kilooctets", false),
            ("kilobyte", "kilobytes", false),
            ("kilobyte", "kilobyte", false),
            ("kilobyte", "kilobytes", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["°C"],
        nouns: [
            ("Grad Celsius", "Grad Celsius", false),
            ("degree Celsius", "degrees Celsius", false),
            ("degré Celsius", "degrés Celsius", false),
            ("grado Celsius", "grados Celsius", false),
            ("grado Celsius", "gradi Celsius", false),
            ("grau Celsius", "graus Celsius", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["°"],
        nouns: [
            ("Grad", "Grad", false),
            ("degree", "degrees", false),
            ("degré", "degrés", false),
            ("grado", "grados", false),
            ("grado", "gradi", false),
            ("grau", "graus", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["Mio."],
        nouns: [
            ("Million", "Millionen", true),
            ("million", "million", false),
            ("million", "millions", false),
            ("millón", "millones", false),
            ("milione", "milioni", false),
            ("milhão", "milhões", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["Mrd."],
        nouns: [
            ("Milliarde", "Milliarden", true),
            ("billion", "billion", false),
            ("milliard", "milliards", false),
            ("mil millones", "mil millones", false),
            ("miliardo", "miliardi", false),
            ("bilhão", "bilhões", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["g"],
        nouns: [
            ("Gramm", "Gramm", false),
            ("gram", "grams", false),
            ("gramme", "grammes", false),
            ("gramo", "gramos", false),
            ("grammo", "grammi", false),
            ("grama", "gramas", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["m"],
        nouns: [
            ("Meter", "Meter", false),
            ("meter", "meters", false),
            ("mètre", "mètres", false),
            ("metro", "metros", false),
            ("metro", "metri", false),
            ("metro", "metros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["l"],
        nouns: [
            ("Liter", "Liter", false),
            ("liter", "liters", false),
            ("litre", "litres", false),
            ("litro", "litros", false),
            ("litro", "litri", false),
            ("litro", "litros", false),
        ],
        currency: false,
    },
    Unit {
        symbols: &["h"],
        nouns: [
            ("Stunde", "Stunden", true),
            ("hour", "hours", false),
            ("heure", "heures", true),
            ("hora", "horas", true),
            ("ora", "ore", true),
            ("hora", "horas", true),
        ],
        currency: false,
    },
];

/// The cents of an amount: "zwölf Euro neunzig".
fn cents_joiner(language: &str) -> &'static str {
    match language {
        "es" => " con ",
        "it" | "pt" => " e ",
        _ => " ",
    }
}

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

    /// Two digits at `i` as a number.
    fn two_digits(&self, i: usize) -> Option<u32> {
        if self.is_digit(i) && self.is_digit(i + 1) {
            Some(self.chars[i].to_digit(10)? * 10 + self.chars[i + 1].to_digit(10)?)
        } else {
            None
        }
    }

    /// One or two digits at `i`: the number and the index after it.
    fn short_number(&self, i: usize) -> Option<(u32, usize)> {
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
    fn date_at(&mut self, i: usize) -> Option<usize> {
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

    fn date_words(&self, day: u32, month: u32, year: Option<i32>) -> String {
        let name = month_name(self.language, month);
        let year = year.map(|y| year_words(self.language, y));
        let with_year = |base: String, joiner: &str| match &year {
            Some(y) => format!("{base}{joiner}{y}"),
            None => base,
        };
        match self.language {
            "de" => with_year(format!("{}en {name}", ordinal_stem_de(day)), " "),
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
    fn time_at(&mut self, i: usize) -> Option<usize> {
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
        if hour > 23 || minute > 59 || self.is_digit(end) {
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
                    hour = to_24(hour, pm);
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
    fn meridiem(&self, i: usize) -> Option<(bool, usize)> {
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

    /// "3." (German), "3rd", "1er", "2º" when the number at hand is an ordinal.
    fn ordinal_at(&mut self, number: &Number, end: usize) -> Option<usize> {
        if number.frac.is_some() || number.grouped {
            return None;
        }
        let n: u32 = number.int.parse().ok()?;
        match self.language {
            "de" => {
                // "am 3. Oktober", "der 2. Platz": a period, a space and a word.
                if self.char(end) != Some('.')
                    || self.char(end + 1) != Some(' ')
                    || !self.char(end + 2).is_some_and(char::is_alphabetic)
                {
                    return None;
                }
                let dative = self.previous_word().is_some_and(|w| {
                    [
                        "am", "vom", "zum", "im", "beim", "dem", "den", "des", "seit",
                    ]
                    .contains(&w.to_lowercase().as_str())
                });
                let ending = if dative { "en" } else { "e" };
                self.out
                    .push_str(&format!("{}{ending}", ordinal_stem_de(n)));
                Some(end + 1)
            }
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

    /// The last word written so far.
    fn previous_word(&self) -> Option<&str> {
        self.out.split_whitespace().last()
    }

    /// The unit or currency whose symbol is at `i`; `before` asks only for signs written before
    /// an amount ("€12").
    fn unit_symbol_at(&self, i: usize, before: bool) -> Option<(&'static Unit, usize)> {
        let mut best: Option<(&'static Unit, usize)> = None;
        for unit in UNITS {
            if before && !unit.currency {
                continue;
            }
            for symbol in unit.symbols {
                if before && symbol.chars().all(char::is_alphabetic) {
                    continue;
                }
                let len = symbol.chars().count();
                if self.followed_by(i, symbol) && best.is_none_or(|(_, end)| i + len > end) {
                    best = Some((unit, i + len));
                }
            }
        }
        best
    }

    fn noun(&self, unit: &Unit) -> Noun {
        let index = LANGUAGES
            .iter()
            .position(|l| *l == self.language)
            .unwrap_or(1);
        unit.nouns[index]
    }

    /// "1,5 km", "1 Stunde", "3 GB".
    fn with_unit(&self, number: &Number, unit: &Unit) -> String {
        if unit.currency {
            return self.amount(number, unit);
        }
        let (one, many, feminine) = self.noun(unit);
        if number.frac.is_none() && number.int == "1" {
            let article = if feminine {
                self.words.one.1
            } else {
                self.words.one.0
            };
            return format!("{article} {one}");
        }
        format!("{} {many}", self.number_words(number))
    }

    /// "12,90 €": "zwölf Euro neunzig"; "0,50 €": "fünfzig Cent".
    fn amount(&self, number: &Number, unit: &Unit) -> String {
        let (one, many, feminine) = self.noun(unit);
        let cents = number
            .frac
            .as_deref()
            .filter(|f| f.len() == 2)
            .and_then(|f| f.parse::<u32>().ok());
        let whole = Number {
            int: number.int.clone(),
            frac: if cents.is_some() {
                None
            } else {
                number.frac.clone()
            },
            grouped: number.grouped,
        };
        let units = if whole.frac.is_none() && whole.int == "1" {
            let article = if feminine {
                self.words.one.1
            } else {
                self.words.one.0
            };
            format!("{article} {one}")
        } else {
            format!("{} {many}", self.number_words(&whole))
        };
        match cents {
            None | Some(0) => units,
            Some(c) if whole.int.trim_start_matches('0').is_empty() && unit.symbols[0] == "€" => {
                format!(
                    "{} {}",
                    cardinal(self.language, c),
                    euro_cents(self.language)
                )
            }
            Some(c) => format!(
                "{units}{}{}",
                cents_joiner(self.language),
                cardinal(self.language, c)
            ),
        }
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

fn to_24(hour: u32, pm: bool) -> u32 {
    match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    }
}

/// The word for a hundredth of a euro.
fn euro_cents(language: &str) -> &'static str {
    match language {
        "de" => "Cent",
        "fr" => "centimes",
        "es" => "céntimos",
        "it" => "centesimi",
        "pt" => "cêntimos",
        _ => "cents",
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

/// Abbreviations and signs of `language` with their words; matched as whole words.
fn abbreviations(language: &str) -> &'static [(&'static str, &'static str)] {
    match language {
        "de" => &[
            ("z. B.", "zum Beispiel"),
            ("z.B.", "zum Beispiel"),
            ("d. h.", "das heißt"),
            ("d.h.", "das heißt"),
            ("u. a.", "unter anderem"),
            ("u.a.", "unter anderem"),
            ("o. Ä.", "oder Ähnliches"),
            ("u. U.", "unter Umständen"),
            ("s. o.", "siehe oben"),
            ("s. u.", "siehe unten"),
            ("usw.", "und so weiter"),
            ("bzw.", "beziehungsweise"),
            ("ca.", "circa"),
            ("Nr.", "Nummer"),
            ("inkl.", "inklusive"),
            ("exkl.", "exklusive"),
            ("ggf.", "gegebenenfalls"),
            ("evtl.", "eventuell"),
            ("vgl.", "vergleiche"),
            ("Dr.", "Doktor"),
            ("Prof.", "Professor"),
            ("Tel.", "Telefon"),
            ("Std.", "Stunden"),
            ("Min.", "Minuten"),
            ("Mio.", "Millionen"),
            ("Mrd.", "Milliarden"),
            ("etc.", "et cetera"),
            ("&", "und"),
        ],
        "fr" => &[
            ("p. ex.", "par exemple"),
            ("c.-à-d.", "c'est-à-dire"),
            ("env.", "environ"),
            ("Mme", "Madame"),
            ("Mlle", "Mademoiselle"),
            ("etc.", "et cetera"),
            ("&", "et"),
        ],
        "es" => &[
            ("p. ej.", "por ejemplo"),
            ("aprox.", "aproximadamente"),
            ("Sr.", "señor"),
            ("Sra.", "señora"),
            ("Dr.", "doctor"),
            ("etc.", "etcétera"),
            ("&", "y"),
        ],
        "it" => &[
            ("ad es.", "ad esempio"),
            ("ecc.", "eccetera"),
            ("Sig.ra", "signora"),
            ("Sig.", "signor"),
            ("Dott.", "dottor"),
            ("&", "e"),
        ],
        "pt" => &[
            ("p. ex.", "por exemplo"),
            ("Sr.", "senhor"),
            ("Sra.", "senhora"),
            ("Dr.", "doutor"),
            ("etc.", "etcétera"),
            ("&", "e"),
        ],
        _ => &[
            ("e.g.", "for example"),
            ("i.e.", "that is"),
            ("etc.", "et cetera"),
            ("vs.", "versus"),
            ("approx.", "approximately"),
            ("Dr.", "Doctor"),
            ("Mrs.", "Missus"),
            ("Mr.", "Mister"),
            ("&", "and"),
        ],
    }
}

fn expand_abbreviations(text: &str, language: &str) -> String {
    let list = abbreviations(language);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < chars.len() {
        let starts = i == 0 || !chars[i - 1].is_alphanumeric();
        if starts {
            for (written, said) in list {
                let len = written.chars().count();
                if chars.len() >= i + len
                    && chars[i..i + len].iter().copied().eq(written.chars())
                    && chars.get(i + len).is_none_or(|c| !c.is_alphanumeric())
                {
                    // "&" only between words: "Müller & Co".
                    if *written == "&"
                        && !(i > 0
                            && chars[i - 1] == ' '
                            && chars.get(i + 1).is_some_and(|c| *c == ' '))
                    {
                        continue;
                    }
                    out.push_str(said);
                    i += len;
                    continue 'outer;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
