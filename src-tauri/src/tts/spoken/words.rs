//! The words numbers need in each language, and the abbreviations written out (FR-022b).

/// The words of `language` that numbers need.
pub(super) struct Words {
    /// The decimal separator as it is spoken ("Komma", "point").
    pub(super) decimal: &'static str,
    pub(super) minus: &'static str,
    pub(super) plus: &'static str,
    /// Between the ends of a range ("10–12": "zehn bis zwölf").
    pub(super) to: &'static str,
    pub(super) percent: &'static str,
    /// "eins" is "ein"/"eine" before a noun: masculine/neuter and feminine.
    pub(super) one: (&'static str, &'static str),
}

pub(super) fn words(language: &str) -> Words {
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

pub(super) fn expand_abbreviations(text: &str, language: &str) -> String {
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
