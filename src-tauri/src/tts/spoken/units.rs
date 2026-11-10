//! Units and currencies after (or a currency sign before) a number: "5 km", "12,90 €" (FR-022b).

use super::{Number, Reader, LANGUAGES};
use crate::assistant::number_words::cardinal;

/// A noun after a number, in one language: the word for one, the word for more, and whether it is
/// feminine (for "eine Stunde", "une heure").
pub(super) type Noun = (&'static str, &'static str, bool);

/// A unit or currency: the ways it is written after (or, for a currency sign, before) a number,
/// and its noun in de, en, fr, es, it, pt.
pub(super) struct Unit {
    symbols: &'static [&'static str],
    nouns: [Noun; 6],
    /// A currency: "12,90 €" says the cents after the unit.
    currency: bool,
}

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

impl Reader<'_> {
    /// The unit or currency whose symbol is at `i`; `before` asks only for signs written before
    /// an amount ("€12").
    pub(super) fn unit_symbol_at(&self, i: usize, before: bool) -> Option<(&'static Unit, usize)> {
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

    pub(super) fn noun(&self, unit: &Unit) -> Noun {
        let index = LANGUAGES
            .iter()
            .position(|l| *l == self.language)
            .unwrap_or(1);
        unit.nouns[index]
    }

    /// "1,5 km", "1 Stunde", "3 GB".
    pub(super) fn with_unit(&self, number: &Number, unit: &Unit) -> String {
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
    pub(super) fn amount(&self, number: &Number, unit: &Unit) -> String {
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
