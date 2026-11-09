//! Words for days, months and clock times, one branch per spoken language (spec 066 FR-022,
//! research R10; T151: native speakers check). `speakable` uses them for ISO dates and times,
//! `tts::spoken` for dates and times written in other ways.

use super::number_words::{cardinal, ordinal_en, ordinal_stem_de, year_words};
use crate::local_time::Civil;

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

/// "am …ten": the German dative ordinal of a day of the month.
fn ordinal_de(d: u32) -> String {
    format!("{}en", ordinal_stem_de(d))
}

/// The name of `month` (1–12) in `language`.
pub fn month_name(language: &str, month: u32) -> &'static str {
    table(&MONTHS, language)[(month.clamp(1, 12) - 1) as usize]
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
