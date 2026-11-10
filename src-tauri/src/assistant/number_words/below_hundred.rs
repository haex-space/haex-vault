//! The words for 0–99, which every larger number is built from.

/// Cardinal number words for 0–99.
pub(super) fn below_hundred(language: &str, n: u32) -> String {
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
