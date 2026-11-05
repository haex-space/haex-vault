//! Spec 081 T024: splitting a sentence into parts.

use super::parts::{plain, split};
use super::spoken::{spoken_protected, PROTECTED_SPACE};

fn words(part: &str) -> usize {
    plain(part).split_whitespace().count()
}

#[test]
fn a_short_sentence_stays_whole() {
    assert_eq!(
        split("Guten Morgen, wie geht es dir?", "de", 20),
        vec!["Guten Morgen, wie geht es dir?"]
    );
    assert!(split("", "de", 20).is_empty());
}

#[test]
fn a_long_sentence_breaks_after_a_comma_into_balanced_parts() {
    let text =
        "Ich habe heute Morgen mit Jonas über das Budget für das nächste Quartal gesprochen, \
                und er meint, dass wir die Ausgaben für Werbung um ein Drittel kürzen sollten, \
                bevor wir neue Leute einstellen und das Büro in der Innenstadt vergrößern.";
    let parts = split(text, "de", 20);
    assert!(parts.len() >= 2, "{parts:?}");
    for part in &parts {
        assert!(words(part) <= 20, "{part}");
    }
    assert!(parts[0].ends_with(','), "{parts:?}");
    assert_eq!(
        parts.join(" "),
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn thirty_words_become_two_parts_of_about_fifteen() {
    let text = (1..=30)
        .map(|i| format!("wort{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let parts = split(&text, "de", 20);
    assert_eq!(parts.len(), 2);
    assert_eq!(words(&parts[0]), 15);
    assert_eq!(words(&parts[1]), 15);
}

#[test]
fn without_punctuation_it_breaks_before_a_conjunction() {
    let text = "Wir treffen uns morgen früh am Bahnhof bei der großen Uhr neben dem Kiosk \
                oder wir gehen direkt zum Café am Marktplatz gegenüber vom alten Rathaus";
    let parts = split(text, "de", 20);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(parts[1].starts_with("oder"), "{parts:?}");
}

#[test]
fn a_written_out_time_and_amount_are_never_split() {
    let raw =
        "Bitte überweise bis zum 04.11.2026 um 14:30 Uhr genau 1.249,90 € an die Stadtwerke, \
               damit der Strom im neuen Büro rechtzeitig vor dem Umzug angemeldet ist und läuft.";
    let protected = spoken_protected(raw, "de");
    assert!(protected.contains(PROTECTED_SPACE));
    for max in [3, 5, 8, 12, 20] {
        let parts: Vec<String> = split(&protected, "de", max)
            .iter()
            .map(|p| plain(p))
            .collect();
        // The time is in one part, whatever the part length.
        assert!(
            parts.iter().any(|p| p.contains("vierzehn Uhr dreißig")),
            "{max}: {parts:?}"
        );
        for part in &parts {
            assert!(!part.contains(PROTECTED_SPACE), "{part}");
        }
    }
}

#[test]
fn a_protected_group_longer_than_a_part_stays_one_part() {
    let group = ["eins"; 25].join(&PROTECTED_SPACE.to_string());
    let parts = split(&format!("Die Nummer lautet {group} und fertig."), "de", 20);
    assert!(parts.iter().any(|p| words(p) == 25), "{parts:?}");
}

#[test]
fn every_language_splits_a_long_sentence() {
    for language in super::LANGUAGES {
        let text = (1..=24)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(split(&text, language, 20).len(), 2, "{language}");
    }
}

/// Spec 081 FR-010 in every language: a long sentence without commas ends a part before one of
/// the language's conjunctions, and a written-out time stays in one part.
#[test]
fn every_language_breaks_before_its_own_conjunctions() {
    let cases = [
        (
            "en",
            "We meet tomorrow morning at the station by the big clock next to the kiosk \
             or we go straight to the cafe on the market square opposite the old town hall",
            "or",
        ),
        (
            "fr",
            "Nous nous retrouvons demain matin à la gare près de la grande horloge du kiosque \
             ou nous allons directement au café de la place du marché en face de la mairie",
            "ou",
        ),
        (
            "es",
            "Nos vemos mañana por la mañana en la estación junto al gran reloj del quiosco \
             o vamos directamente al café de la plaza del mercado frente al viejo ayuntamiento",
            "o",
        ),
        (
            "it",
            "Ci vediamo domani mattina alla stazione vicino al grande orologio del chiosco \
             ma prima passiamo al bar della piazza del mercato di fronte al vecchio municipio",
            "ma",
        ),
        (
            "pt",
            "Encontramo-nos amanhã de manhã na estação junto ao grande relógio do quiosque \
             mas antes passamos pelo café da praça do mercado em frente à antiga câmara",
            "mas",
        ),
    ];
    for (language, text, conjunction) in cases {
        let parts = split(text, language, 20);
        assert_eq!(parts.len(), 2, "{language}: {parts:?}");
        assert!(
            parts[1].starts_with(&format!("{conjunction} ")),
            "{language}: {parts:?}"
        );
    }
}

#[test]
fn a_written_out_time_stays_whole_in_every_language() {
    for language in super::LANGUAGES {
        let raw = "Alpha beta gamma delta epsilon zeta eta theta iota kappa 14:30 lambda my nu xi \
                   omikron pi rho sigma tau ypsilon phi chi psi omega";
        let protected = spoken_protected(raw, language);
        let time: String = spoken_protected("14:30", language);
        let time = plain(&time);
        for max in [4, 8, 12, 20] {
            let parts: Vec<String> = split(&protected, language, max)
                .iter()
                .map(|p| plain(p))
                .collect();
            assert!(
                parts.iter().any(|p| p.contains(&time)),
                "{language} {max}: {time:?} not whole in {parts:?}"
            );
        }
    }
}

#[test]
fn a_bare_hour_keeps_its_uhr() {
    let protected = spoken_protected("Treffen um 14 Uhr und um 1 Uhr nachts.", "de");
    let joined = |words: &[&str]| words.join(&PROTECTED_SPACE.to_string());
    assert!(
        protected.contains(&joined(&["vierzehn", "Uhr"])),
        "{protected}"
    );
    assert!(protected.contains(&joined(&["ein", "Uhr"])), "{protected}");
    assert_eq!(
        plain(&protected),
        "Treffen um vierzehn Uhr und um ein Uhr nachts."
    );
}
