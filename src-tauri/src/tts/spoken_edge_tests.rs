//! Spec 066 FR-022b (T167): the cases a first reading got wrong (telephone numbers read as
//! ranges, a sentence-final number read as an ordinal, half-read tokens, counts read as years,
//! counts above four billion), and odd or long input, which must never panic or slow down.

use super::spoken::spoken;
use crate::assistant::number_words::cardinal_large;

fn check(language: &str, cases: &[(&str, &str)]) {
    for (written, said) in cases {
        assert_eq!(spoken(written, language), *said, "{language}: {written}");
    }
}

#[test]
fn german_numbers_in_context() {
    check(
        "de",
        &[
            (
                "Ruf 0711-123456 an.",
                "Ruf null sieben eins eins, eins zwei drei vier fünf sechs an.",
            ),
            (
                "Das sind 42. Danach geht es weiter.",
                "Das sind zweiundvierzig. Danach geht es weiter.",
            ),
            (
                "Sie hat Platz 1. Wir gratulieren.",
                "Sie hat Platz eins. Wir gratulieren.",
            ),
            ("Er ist 3. geworden.", "Er ist dritte geworden."),
            ("Im 20. Jahrhundert.", "Im zwanzigsten Jahrhundert."),
            (
                "Heute ist der 04.11.2026.",
                "Heute ist der vierte November zweitausendsechsundzwanzig.",
            ),
            (
                "Im Zeitraum 2025-2026.",
                "Im Zeitraum zweitausendfünfundzwanzig bis zweitausendsechsundzwanzig.",
            ),
            (
                "Um 24:00 Uhr ist Schluss.",
                "Um vierundzwanzig Uhr ist Schluss.",
            ),
            (
                "Es sind 5.000.000.000 Menschen.",
                "Es sind fünf Milliarden Menschen.",
            ),
            // No safe reading of the whole: it stays as written, not half read.
            ("Das Spiel endete 2:1.", "Das Spiel endete 2:1."),
            ("Ein 1/2 Liter.", "Ein 1/2 Liter."),
        ],
    );
}

#[test]
fn english_numbers_in_context() {
    check(
        "en",
        &[
            (
                "Call 555-123-4567 now.",
                "Call five five five, one two three, four five six seven now.",
            ),
            (
                "I found 1234 files.",
                "I found one thousand two hundred thirty-four files.",
            ),
            (
                "1999 was a good year.",
                "nineteen ninety-nine was a good year.",
            ),
            (
                "In March 2026 we start.",
                "In March twenty twenty-six we start.",
            ),
            ("Since 1990 it grew.", "Since nineteen ninety it grew."),
            ("Meet at 13:30 pm.", "Meet at one thirty in the afternoon."),
            ("On 11/04/2026.", "On 11/04/2026."),
            ("About 8,000,000,000 people.", "About eight billion people."),
            ("The score was 3-1.", "The score was three to one."),
        ],
    );
    check(
        "es",
        &[(
            "Hay 5.000.000.000 personas.",
            "Hay cinco mil millones personas.",
        )],
    );
}

#[test]
fn counts_above_four_billion_have_words_up_to_a_trillion() {
    assert_eq!(
        cardinal_large("de", 12_345_678_901).as_deref(),
        Some("zwölf Milliarden dreihundertfünfundvierzig Millionen sechshundertachtundsiebzigtausend neunhunderteins")
    );
    assert_eq!(
        cardinal_large("en", 999_000_000_000).as_deref(),
        Some("nine hundred ninety-nine billion")
    );
    assert_eq!(
        cardinal_large("fr", 7_000_000_000).as_deref(),
        Some("sept milliards")
    );
    assert_eq!(cardinal_large("de", 1_000_000_000_000), None);
}

/// A small deterministic generator, so the odd inputs are the same on every run.
fn odd_inputs() -> Vec<String> {
    const PIECES: &[&str] = &[
        "0",
        "1",
        "9",
        "12",
        "2026",
        "99999999999999999999",
        ".",
        ",",
        ":",
        "/",
        "-",
        "−",
        "–",
        "+",
        "%",
        "€",
        "$",
        "°",
        "º",
        "ª",
        " ",
        "\u{a0}",
        "Uhr",
        "pm",
        "a.m.",
        "km/h",
        "Mio.",
        "z. B.",
        "&",
        "er",
        "e",
        "st",
        "h",
        "ü",
        "日本",
        "😀",
        "\n",
        "(",
        "am",
        "der",
    ];
    let mut state: u64 = 0x2026_1104;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap_or(0)
    };
    (0..3_000)
        .map(|_| {
            let len = next() % 12;
            (0..len).map(|_| PIECES[next() % PIECES.len()]).collect()
        })
        .collect()
}

#[test]
fn odd_input_never_panics_in_any_language() {
    for text in odd_inputs() {
        for language in ["de", "en", "fr", "es", "it", "pt", "xx"] {
            let said = spoken(&text, language);
            // Nothing is lost: text without digits or abbreviations comes back as it was.
            if !text.chars().any(|c| c.is_ascii_digit()) && !text.contains(['&', '.']) {
                assert_eq!(said, text, "{language}: {text:?}");
            }
        }
    }
}

#[test]
fn a_very_long_digit_run_is_said_digit_by_digit() {
    let said = spoken(&"7".repeat(100_000), "de");
    assert_eq!(said.split(' ').count(), 100_000);
    assert!(said.split(' ').all(|w| w == "sieben"));
}

#[test]
fn a_long_text_is_read_like_its_parts() {
    let part = "Am 04.11.2026 um 14:30 Uhr kostet es 12,90 € (ca. 3,5 % mehr). ";
    let one = spoken(part, "de");
    assert_eq!(spoken(&part.repeat(2_000), "de"), one.repeat(2_000));
}
