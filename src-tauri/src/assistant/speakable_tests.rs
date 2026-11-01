//! Spec 066 T053: spoken answers carry no formatting, point to the chat for tables, code and links,
//! say dates and times as words and are cut when long.

use super::speakable::{
    cardinal, day_phrase, speak_all, time_phrase, Civil, Clock, Speakable, Speech,
};

/// Wednesday, 28 October 2026, 15:00, one hour ahead of UTC.
fn clock() -> Clock {
    Clock {
        now: Civil::new(2026, 10, 28, 15, 0),
        utc_offset_minutes: 60,
    }
}

fn say(language: &str, text: &str) -> Vec<String> {
    speak_all(text, language, clock()).sentences
}

fn one(language: &str, text: &str) -> String {
    let spoken = say(language, text);
    assert_eq!(spoken.len(), 1, "{spoken:?}");
    spoken.into_iter().next().unwrap()
}

#[test]
fn formatting_marks_are_not_spoken() {
    assert_eq!(one("de", "## Überschrift"), "Überschrift");
    assert_eq!(
        one(
            "de",
            "**Wichtig:** das ist *kursiv*, ~~alt~~ und `code` mit snake_case."
        ),
        "Wichtig: das ist kursiv, alt und code mit snake_case."
    );
    assert_eq!(
        say(
            "de",
            "- Milch\n* Eier\n1. Brot\n2) Butter\n> Zitat\n- [x] erledigt"
        ),
        ["Milch", "Eier", "Brot", "Soll ich mehr vorlesen?"]
    );
    assert_eq!(
        one("de", "Siehe [die Seite](https://x.y/z) und ![Bild](a.png)."),
        "Siehe die Seite und Bild."
    );
    assert!(say("de", "---\n***\n").is_empty());
}

#[test]
fn tables_code_and_links_point_to_the_chat() {
    assert_eq!(
        say(
            "de",
            "Hier die Übersicht:\n| A | B |\n|---|---|\n| 1 | 2 |\nFertig."
        ),
        [
            "Hier die Übersicht:",
            "Die Tabelle habe ich dir im Chat gezeigt.",
            "Fertig."
        ]
    );
    assert_eq!(
        say("en", "```bash\nrm -rf build\nls\n```\nDone."),
        ["I've put the code in the chat.", "Done."]
    );
    assert_eq!(
        one("de", "Mehr dazu unter https://example.com/a?b=1."),
        "Mehr dazu unter Link im Chat."
    );
    assert_eq!(
        one("en", "See (www.example.com) for details."),
        "See (the link in the chat) for details."
    );
}

#[test]
fn iso_date_times_are_spoken_relative_to_now() {
    assert_eq!(one("de", "2026-10-29T10:00"), "morgen um zehn Uhr");
    assert_eq!(one("en", "2026-10-29T10:00"), "tomorrow at ten");
    assert_eq!(
        one("de", "Der Termin ist am 2026-10-29T10:00."),
        "Der Termin ist morgen um zehn Uhr."
    );
    assert_eq!(
        one("en", "It starts at 2026-10-28T14:30:00."),
        "It starts today at two thirty in the afternoon."
    );
    assert_eq!(one("de", "2026-11-02T09:00"), "am Montag um neun Uhr");
    assert_eq!(one("en", "2026-10-27"), "yesterday");
    assert_eq!(one("de", "2026-10-30"), "übermorgen");
}

#[test]
fn a_time_with_an_offset_is_spoken_in_local_time() {
    assert_eq!(one("de", "2026-10-29T08:00Z"), "morgen um neun Uhr");
    assert_eq!(one("de", "2026-10-29T08:00+02:00"), "morgen um sieben Uhr");
    // Crossing midnight changes the day.
    assert_eq!(
        one("en", "2026-10-28T23:30Z"),
        "tomorrow at twelve thirty at night"
    );
}

#[test]
fn dates_beyond_the_week_name_day_and_month_and_a_different_year() {
    let today = clock().now;
    let date = Civil::new(2026, 12, 24, 0, 0);
    let cases = [
        ("de", "am vierundzwanzigsten Dezember"),
        ("en", "on December twenty-fourth"),
        ("fr", "le vingt-quatre décembre"),
        ("es", "el veinticuatro de diciembre"),
        ("it", "il ventiquattro dicembre"),
        ("pt", "em vinte e quatro de dezembro"),
    ];
    for (language, expected) in cases {
        assert_eq!(day_phrase(language, date, today), expected, "{language}");
    }
    let next_year = Civil::new(2027, 3, 1, 0, 0);
    assert_eq!(
        day_phrase("de", next_year, today),
        "am ersten März zweitausendsiebenundzwanzig"
    );
    assert_eq!(
        day_phrase("en", next_year, today),
        "on March first, twenty twenty-seven"
    );
    assert_eq!(
        day_phrase("fr", next_year, today),
        "le premier mars deux mille vingt-sept"
    );
    assert_eq!(
        day_phrase("it", next_year, today),
        "il primo marzo duemilaventisette"
    );
    assert_eq!(
        day_phrase("pt", next_year, today),
        "em primeiro de março de dois mil e vinte e sete"
    );
}

#[test]
fn clock_times_are_spoken_in_every_language() {
    assert_eq!(
        one("de", "Wir treffen uns um 10:30 Uhr."),
        "Wir treffen uns um zehn Uhr dreißig."
    );
    assert_eq!(
        one("de", "Um 1:05 ist Schluss."),
        "Um ein Uhr fünf ist Schluss."
    );
    assert_eq!(one("en", "Meet me at 7:05."), "Meet me at seven oh five.");
    assert_eq!(
        one("en", "The train leaves at 18:45."),
        "The train leaves at six forty-five in the evening."
    );
    assert_eq!(
        one("fr", "2026-10-29T21:00"),
        "demain à vingt et une heures"
    );
    assert_eq!(one("fr", "2026-10-29T12:15"), "demain à midi quinze");
    assert_eq!(one("es", "2026-10-29T13:00"), "mañana a las trece");
    assert_eq!(one("es", "2026-10-29T01:15"), "mañana a la una y quince");
    assert_eq!(one("it", "2026-10-29T01:00"), "domani all'una");
    assert_eq!(one("it", "2026-10-29T08:00"), "domani alle otto");
    assert_eq!(one("pt", "2026-10-29T01:00"), "amanhã à uma");
    assert_eq!(
        one("pt", "2026-10-29T22:30"),
        "amanhã às vinte e duas e trinta"
    );
    assert_eq!(time_phrase("en", 0, 0, true), "at midnight");
    assert_eq!(time_phrase("en", 12, 0, false), "noon");
}

#[test]
fn ratios_versions_and_ports_are_no_times() {
    assert_eq!(
        one("en", "Use 16:9 on localhost:8080 with v1.2:30x."),
        "Use 16:9 on localhost:8080 with v1.2:30x."
    );
}

#[test]
fn number_words_follow_each_language() {
    let cases = [
        ("de", 21, "einundzwanzig"),
        ("de", 1, "eins"),
        ("en", 45, "forty-five"),
        ("fr", 71, "soixante et onze"),
        ("fr", 80, "quatre-vingts"),
        ("fr", 91, "quatre-vingt-onze"),
        ("es", 22, "veintidós"),
        ("es", 31, "treinta y uno"),
        ("it", 28, "ventotto"),
        ("it", 23, "ventitré"),
        ("pt", 16, "dezesseis"),
        ("pt", 47, "quarenta e sete"),
    ];
    for (language, n, expected) in cases {
        assert_eq!(cardinal(language, n), expected, "{language} {n}");
    }
}

#[test]
fn a_long_answer_is_cut_after_three_sentences_with_an_offer_to_read_on() {
    let text = "Eins ist erledigt. Zwei auch. Drei ebenso. Vier folgt. Fünf zuletzt.";
    let spoken = speak_all(text, "de", clock());
    assert_eq!(
        spoken.sentences,
        [
            "Eins ist erledigt.",
            "Zwei auch.",
            "Drei ebenso.",
            "Soll ich mehr vorlesen?"
        ]
    );
    assert_eq!(spoken.rest, ["Vier folgt.", "Fünf zuletzt."]);
}

#[test]
fn four_sentences_are_spoken_whole() {
    let spoken = speak_all("Eins. Zwei. Drei. Vier.", "en", clock());
    assert_eq!(spoken.sentences, ["Eins.", "Zwei.", "Drei.", "Vier."]);
    assert!(spoken.rest.is_empty());
}

#[test]
fn streamed_answers_are_spoken_sentence_by_sentence() {
    let mut speech = Speech::new("en", clock());
    let mut heard = Vec::new();
    for delta in [
        "The **meeting** is ",
        "at 2026-10-29T10:00. ",
        "Bring the",
        " slides.",
    ] {
        heard.push(speech.push(delta));
    }
    heard.push(speech.finish());
    assert_eq!(
        heard,
        [
            vec![],
            vec![],
            vec!["The meeting is tomorrow at ten.".to_owned()],
            vec![],
            vec!["Bring the slides.".to_owned()],
        ]
    );
}

#[test]
fn an_unknown_language_speaks_english() {
    let mut lines = Speakable::new("xx", clock());
    assert_eq!(
        lines.line("| a |").as_deref(),
        Some("I've put the table in the chat.")
    );
}
