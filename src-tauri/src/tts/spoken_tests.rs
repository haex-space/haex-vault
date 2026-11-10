//! Spec 066 FR-022b (T167): what the speech model is given has numbers, times, dates, amounts,
//! units, ranges, ordinals and abbreviations as words of the voice's language.

use super::spoken::spoken;
use crate::assistant::number_words::cardinal;

fn check(language: &str, cases: &[(&str, &str)]) {
    for (written, said) in cases {
        assert_eq!(spoken(written, language), *said, "{language}: {written}");
    }
}

#[test]
fn german_times_dates_and_amounts_are_words() {
    check(
        "de",
        &[
            (
                "Der Termin ist um 14:30 Uhr.",
                "Der Termin ist um vierzehn Uhr dreißig.",
            ),
            ("Treffen um 14 Uhr.", "Treffen um vierzehn Uhr."),
            ("Bis 1 Uhr nachts.", "Bis ein Uhr nachts."),
            ("Start um 9.30 Uhr.", "Start um neun Uhr dreißig."),
            ("Offen von 10-12 Uhr.", "Offen von zehn bis zwölf Uhr."),
            ("Das kostet 12,90 €.", "Das kostet zwölf Euro neunzig."),
            ("Nur 0,50 €.", "Nur fünfzig Cent."),
            ("Genau 1 € mehr.", "Genau ein Euro mehr."),
            ("Kostet €5.", "Kostet fünf Euro."),
            (
                "Die Quote liegt bei 3,5 %.",
                "Die Quote liegt bei drei Komma fünf Prozent.",
            ),
            (
                "Es sind 1.234 Kilometer.",
                "Es sind eintausend zweihundertvierunddreißig Kilometer.",
            ),
            ("Noch 5 km.", "Noch fünf Kilometer."),
            ("Dauert 1 h.", "Dauert eine Stunde."),
            ("In 10 min.", "In zehn Minuten."),
            ("Heute -5 °C.", "Heute minus fünf Grad Celsius."),
            ("Am 3. Oktober.", "Am dritten Oktober."),
            ("Der 2. Platz.", "Der zweite Platz."),
            (
                "Am 04.11.2026 geht es los.",
                "Am vierten November zweitausendsechsundzwanzig geht es los.",
            ),
            (
                "Seit 2026-11-04 offen.",
                "Seit vierten November zweitausendsechsundzwanzig offen.",
            ),
            (
                "Im Jahr 1999 war das so.",
                "Im Jahr neunzehnhundertneunundneunzig war das so.",
            ),
            ("Das kostet 5.", "Das kostet fünf."),
            (
                "Es gibt 2,5 Mio. Fälle.",
                "Es gibt zwei Komma fünf Millionen Fälle.",
            ),
        ],
    );
}

#[test]
fn german_abbreviations_and_signs_are_words() {
    check(
        "de",
        &[
            ("z. B. Äpfel", "zum Beispiel Äpfel"),
            ("Das ist bzw. war so.", "Das ist beziehungsweise war so."),
            ("Müller & Co", "Müller und Co"),
            ("Nr. 5", "Nummer fünf"),
            // Not an abbreviation: "ca" inside a word.
            ("Cafeteria", "Cafeteria"),
        ],
    );
}

#[test]
fn telephone_numbers_are_said_digit_by_digit() {
    check(
        "de",
        &[
            (
                "Ruf 0176 123456 an.",
                "Ruf null eins sieben sechs eins zwei drei vier fünf sechs an.",
            ),
            ("Unter +49 erreichbar.", "Unter plus vier neun erreichbar."),
        ],
    );
}

#[test]
fn what_is_no_number_stays_as_written() {
    check(
        "de",
        &[
            ("Version 1.2.3 und 4K.", "Version 1.2.3 und 4K."),
            (
                "Die IP 192.168.0.1 antwortet.",
                "Die IP 192.168.0.1 antwortet.",
            ),
            ("Datei mp3 und H2O.", "Datei mp3 und H2O."),
            ("Kein Text ohne Zahlen.", "Kein Text ohne Zahlen."),
        ],
    );
}

#[test]
fn english_times_amounts_and_ordinals_are_words() {
    check(
        "en",
        &[
            ("Meet at 2:30 pm.", "Meet at two thirty in the afternoon."),
            ("It costs $12.50.", "It costs twelve dollars fifty."),
            ("The 3rd time.", "The third time."),
            (
                "About 1,500 people.",
                "About one thousand five hundred people.",
            ),
            ("Back in 1999.", "Back in nineteen ninety-nine."),
            ("Fruit, e.g. apples.", "Fruit, for example apples."),
            ("Pi is 3.14.", "Pi is three point one four."),
        ],
    );
}

#[test]
fn french_spanish_italian_and_portuguese_are_words() {
    check(
        "fr",
        &[
            (
                "Rendez-vous à 14h30.",
                "Rendez-vous à quatorze heures trente.",
            ),
            (
                "Cela coûte 12,90 €.",
                "Cela coûte douze euros quatre-vingt-dix.",
            ),
            ("Le 1er mai.", "Le premier mai."),
            ("Le 2e étage.", "Le deuxième étage."),
            ("Le 4e jour.", "Le quatrième jour."),
        ],
    );
    check(
        "es",
        &[
            ("Sube un 3,5 %.", "Sube un tres coma cinco por ciento."),
            ("Cuesta 12,90 €.", "Cuesta doce euros con noventa."),
            ("El 2º puesto.", "El segundo puesto."),
        ],
    );
    check(
        "it",
        &[
            ("Sono 108 persone.", "Sono centotto persone."),
            ("Costa 12,90 €.", "Costa dodici euro e novanta."),
        ],
    );
    check(
        "pt",
        &[
            ("São 1.100 pessoas.", "São mil e cem pessoas."),
            ("São 1.230 pessoas.", "São mil duzentos e trinta pessoas."),
        ],
    );
}

#[test]
fn an_unknown_language_is_read_as_english() {
    assert_eq!(spoken("At 5 pm.", "pl"), "At five pm.");
}

#[test]
fn large_numbers_follow_each_language() {
    let cases = [
        ("de", 101, "einhunderteins"),
        ("de", 1_001, "eintausend eins"),
        ("de", 1_000_000, "eine Million"),
        ("de", 2_500_000, "zwei Millionen fünfhunderttausend"),
        ("en", 1_234, "one thousand two hundred thirty-four"),
        ("en", 3_000_000_000, "three billion"),
        ("fr", 200, "deux cents"),
        ("fr", 201, "deux cent un"),
        ("fr", 2_000, "deux mille"),
        ("fr", 80_000, "quatre-vingt mille"),
        ("es", 100, "cien"),
        ("es", 101, "ciento uno"),
        ("es", 500, "quinientos"),
        ("es", 21_000, "veintiún mil"),
        ("es", 1_000_000, "un millón"),
        ("it", 1_000, "mille"),
        ("it", 2_000, "duemila"),
        ("it", 180, "centottanta"),
        ("pt", 100, "cem"),
        ("pt", 2_000_000, "dois milhões"),
    ];
    for (language, n, expected) in cases {
        assert_eq!(cardinal(language, n), expected, "{language} {n}");
    }
}
