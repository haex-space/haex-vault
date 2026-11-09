//! Number words, one branch per spoken language (spec 066 FR-022, FR-022b; T151: native speakers
//! check): counts up to the billions, ordinals and years.

mod below_hundred;

use below_hundred::below_hundred;

/// Cardinal number words in `language`, up to the billions (spec 066 FR-022b). Groups of thousands
/// are separate words ("zweitausend dreihundertvierundfünfzig"), which the speech model reads more
/// steadily than one long compound.
pub fn cardinal(language: &str, n: u32) -> String {
    if n < 100 {
        return below_hundred(language, n);
    }
    let billions = n / 1_000_000_000;
    let millions = n / 1_000_000 % 1_000;
    let thousands = n / 1_000 % 1_000;
    let rest = n % 1_000;
    let mut parts: Vec<String> = Vec::new();
    match language {
        "de" => {
            let big = |count: u32, one: &str, many: &str| {
                if count == 1 {
                    format!("eine {one}")
                } else {
                    format!("{} {many}", hundreds_de(count, true))
                }
            };
            if billions > 0 {
                parts.push(big(billions, "Milliarde", "Milliarden"));
            }
            if millions > 0 {
                parts.push(big(millions, "Million", "Millionen"));
            }
            if thousands > 0 {
                parts.push(format!("{}tausend", hundreds_de(thousands, false)));
            }
            if rest > 0 {
                parts.push(hundreds_de(rest, true));
            }
        }
        "fr" => {
            let big = |count: u32, one: &str, many: &str| {
                if count == 1 {
                    format!("un {one}")
                } else {
                    format!("{} {many}", hundreds_fr(count, true))
                }
            };
            if billions > 0 {
                parts.push(big(billions, "milliard", "milliards"));
            }
            if millions > 0 {
                parts.push(big(millions, "million", "millions"));
            }
            match thousands {
                0 => {}
                1 => parts.push("mille".to_owned()),
                t => parts.push(format!("{} mille", hundreds_fr(t, false))),
            }
            if rest > 0 {
                parts.push(hundreds_fr(rest, true));
            }
        }
        "es" => {
            // Spanish counts the billions as "mil millones".
            let all_millions = n / 1_000_000;
            match all_millions {
                0 => {}
                1 => parts.push("un millón".to_owned()),
                m => parts.push(format!("{} millones", apocope_es(&cardinal("es", m)))),
            }
            match thousands {
                0 => {}
                1 => parts.push("mil".to_owned()),
                t => parts.push(format!("{} mil", apocope_es(&hundreds_es(t)))),
            }
            if rest > 0 {
                parts.push(hundreds_es(rest));
            }
        }
        "it" => {
            let big = |count: u32, one: &str, many: &str| {
                if count == 1 {
                    format!("un {one}")
                } else {
                    format!("{} {many}", hundreds_it(count))
                }
            };
            if billions > 0 {
                parts.push(big(billions, "miliardo", "miliardi"));
            }
            if millions > 0 {
                parts.push(big(millions, "milione", "milioni"));
            }
            match thousands {
                0 => {}
                1 => parts.push("mille".to_owned()),
                t => parts.push(format!("{}mila", hundreds_it(t))),
            }
            if rest > 0 {
                parts.push(hundreds_it(rest));
            }
        }
        "pt" => {
            let big = |count: u32, one: &str, many: &str| {
                if count == 1 {
                    format!("um {one}")
                } else {
                    format!("{} {many}", hundreds_pt(count))
                }
            };
            if billions > 0 {
                parts.push(big(billions, "bilhão", "bilhões"));
            }
            if millions > 0 {
                parts.push(big(millions, "milhão", "milhões"));
            }
            match thousands {
                0 => {}
                1 => parts.push("mil".to_owned()),
                t => parts.push(format!("{} mil", hundreds_pt(t))),
            }
            if rest > 0 {
                // "mil e cem", "dois mil e vinte", but "mil duzentos e trinta".
                let joined = hundreds_pt(rest);
                if !parts.is_empty() && (rest < 100 || rest.is_multiple_of(100)) {
                    parts.push(format!("e {joined}"));
                } else {
                    parts.push(joined);
                }
            }
        }
        _ => {
            let groups = [
                (billions, " billion"),
                (millions, " million"),
                (thousands, " thousand"),
                (rest, ""),
            ];
            for (count, name) in groups {
                if count > 0 {
                    parts.push(format!("{}{name}", hundreds_en(count)));
                }
            }
        }
    }
    parts.join(" ")
}

/// German 1–999; `last` says whether the number ends here ("hunderteins") or a word follows
/// ("hundertein…tausend").
fn hundreds_de(n: u32, last: bool) -> String {
    let (h, r) = (n / 100, n % 100);
    let mut out = String::new();
    if h > 0 {
        out.push_str(if h == 1 { "ein" } else { "" });
        if h > 1 {
            out.push_str(&below_hundred("de", h));
        }
        out.push_str("hundert");
    }
    match r {
        0 => {}
        1 if !last => out.push_str("ein"),
        r => out.push_str(&below_hundred("de", r)),
    }
    out
}

/// French 1–999; `last` keeps the plural "s" of "deux cents" and "quatre-vingts", which a
/// following "mille" or noun drops.
fn hundreds_fr(n: u32, last: bool) -> String {
    let (h, r) = (n / 100, n % 100);
    let tail = match r {
        0 => String::new(),
        80 if !last => "quatre-vingt".to_owned(),
        r => below_hundred("fr", r),
    };
    let head = match h {
        0 => return tail,
        1 => "cent".to_owned(),
        h if r == 0 && last => format!("{} cents", below_hundred("fr", h)),
        h => format!("{} cent", below_hundred("fr", h)),
    };
    if tail.is_empty() {
        head
    } else {
        format!("{head} {tail}")
    }
}

/// Spanish 1–999.
fn hundreds_es(n: u32) -> String {
    const HUNDREDS: [&str; 10] = [
        "",
        "ciento",
        "doscientos",
        "trescientos",
        "cuatrocientos",
        "quinientos",
        "seiscientos",
        "setecientos",
        "ochocientos",
        "novecientos",
    ];
    let (h, r) = (n / 100, n % 100);
    match (h, r) {
        (0, r) => below_hundred("es", r),
        (1, 0) => "cien".to_owned(),
        (h, 0) => HUNDREDS[h as usize].to_owned(),
        (h, r) => format!("{} {}", HUNDREDS[h as usize], below_hundred("es", r)),
    }
}

/// "uno" becomes "un" before "mil" and "millones" ("veintiún mil", "un millón").
fn apocope_es(words: &str) -> String {
    if let Some(stem) = words.strip_suffix("veintiuno") {
        format!("{stem}veintiún")
    } else if let Some(stem) = words.strip_suffix("uno") {
        format!("{stem}un")
    } else {
        words.to_owned()
    }
}

/// Italian 1–999, one word: "centotto", "duecentoventi".
fn hundreds_it(n: u32) -> String {
    let (h, r) = (n / 100, n % 100);
    let mut out = match h {
        0 => String::new(),
        1 => "cento".to_owned(),
        h => format!("{}cento", below_hundred("it", h)),
    };
    if r > 0 {
        // "cento" + "otto"/"ottanta" loses its last vowel.
        if h > 0 && (r == 8 || (80..=89).contains(&r)) {
            out.pop();
        }
        out.push_str(&below_hundred("it", r));
    }
    out
}

/// Portuguese 1–999.
fn hundreds_pt(n: u32) -> String {
    const HUNDREDS: [&str; 10] = [
        "",
        "cento",
        "duzentos",
        "trezentos",
        "quatrocentos",
        "quinhentos",
        "seiscentos",
        "setecentos",
        "oitocentos",
        "novecentos",
    ];
    let (h, r) = (n / 100, n % 100);
    match (h, r) {
        (0, r) => below_hundred("pt", r),
        (1, 0) => "cem".to_owned(),
        (h, 0) => HUNDREDS[h as usize].to_owned(),
        (h, r) => format!("{} e {}", HUNDREDS[h as usize], below_hundred("pt", r)),
    }
}

/// English 1–999.
fn hundreds_en(n: u32) -> String {
    let (h, r) = (n / 100, n % 100);
    match (h, r) {
        (0, r) => below_hundred("en", r),
        (h, 0) => format!("{} hundred", below_hundred("en", h)),
        (h, r) => format!(
            "{} hundred {}",
            below_hundred("en", h),
            below_hundred("en", r)
        ),
    }
}

/// [`cardinal`] for counts up to 999 billion ("5.000.000.000"); `None` above.
pub fn cardinal_large(language: &str, n: u64) -> Option<String> {
    if let Ok(small) = u32::try_from(n) {
        return Some(cardinal(language, small));
    }
    if n >= 1_000_000_000_000 {
        return None;
    }
    if language == "es" {
        // "cinco mil millones": Spanish counts the billions in millions.
        let millions = u32::try_from(n / 1_000_000).ok()?;
        let rest = u32::try_from(n % 1_000_000).ok()?;
        let head = format!("{} millones", apocope_es(&cardinal("es", millions)));
        return Some(with_rest(head, language, rest));
    }
    // More than four billion: always the plural.
    let billions = u32::try_from(n / 1_000_000_000).ok()?;
    let rest = u32::try_from(n % 1_000_000_000).ok()?;
    let head = match language {
        "de" => format!("{} Milliarden", hundreds_de(billions, true)),
        "fr" => format!("{} milliards", hundreds_fr(billions, true)),
        "it" => format!("{} miliardi", hundreds_it(billions)),
        "pt" => format!("{} bilhões", hundreds_pt(billions)),
        _ => format!("{} billion", hundreds_en(billions)),
    };
    Some(with_rest(head, language, rest))
}

fn with_rest(head: String, language: &str, rest: u32) -> String {
    if rest == 0 {
        head
    } else {
        format!("{head} {}", cardinal(language, rest))
    }
}

/// The stem of a German ordinal, to which "e", "en", "er" are added: "erst", "dritt",
/// "zwanzigst", "einhundertst".
pub fn ordinal_stem_de(n: u32) -> String {
    match n % 100 {
        _ if n >= 100 && n.is_multiple_of(100) => format!("{}st", cardinal("de", n)),
        r if n >= 100 => {
            let head = cardinal("de", n - r);
            let head = head.replace(' ', "");
            format!("{head}{}", ordinal_stem_de(r))
        }
        1 => "erst".to_owned(),
        3 => "dritt".to_owned(),
        7 => "siebt".to_owned(),
        8 => "acht".to_owned(),
        2..=19 => format!("{}t", cardinal("de", n)),
        _ => format!("{}st", cardinal("de", n)),
    }
}

pub fn ordinal_en(d: u32) -> String {
    if d >= 100 {
        let r = d % 100;
        return if r == 0 {
            format!("{}th", cardinal("en", d))
        } else {
            format!("{} {}", cardinal("en", d - r), ordinal_en(r))
        };
    }
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

/// A year in words: German and English say the years before 2000 in hundreds
/// ("neunzehnhundertneunundneunzig", "nineteen ninety-nine"); the others count them.
pub fn year_words(language: &str, year: i32) -> String {
    let Ok(y) = u32::try_from(year) else {
        return year.to_string();
    };
    match (language, y) {
        ("de", 1100..=1999) => {
            let r = y % 100;
            let rest = if r == 0 {
                String::new()
            } else {
                cardinal("de", r)
            };
            format!("{}hundert{rest}", cardinal("de", y / 100))
        }
        ("de", 2000..=2099) => format!("zweitausend{}", hundreds_de(y - 2000, true)),
        ("en", 1100..=1999) | ("en", 2010..=2099) => {
            let r = y % 100;
            let rest = match r {
                0 => " hundred".to_owned(),
                1..=9 => format!(" oh {}", cardinal("en", r)),
                _ => format!(" {}", cardinal("en", r)),
            };
            format!("{}{rest}", cardinal("en", y / 100))
        }
        ("pt", 2001..=2099) => format!("dois mil e {}", cardinal("pt", y - 2000)),
        ("it", 2001..=2099) => format!("duemila{}", cardinal("it", y - 2000)),
        _ => cardinal(language, y),
    }
}
