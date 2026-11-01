//! Spec 066 T035: the streamed answer is cut at sentence ends, not inside numbers, abbreviations or
//! URLs.

use super::sentences::Sentences;

fn split(text: &str) -> Vec<String> {
    let mut s = Sentences::new();
    let mut out = s.push(text);
    out.extend(s.finish());
    out
}

/// Feeds `text` one character at a time, as the slowest possible token stream.
fn split_streamed(text: &str) -> Vec<String> {
    let mut s = Sentences::new();
    let mut out = Vec::new();
    for c in text.chars() {
        out.extend(s.push(&c.to_string()));
    }
    out.extend(s.finish());
    out
}

fn both(text: &str) -> Vec<String> {
    let whole = split(text);
    assert_eq!(split_streamed(text), whole, "streamed differently: {text}");
    whole
}

#[test]
fn cuts_at_sentence_marks_and_line_breaks() {
    assert_eq!(
        both("Hallo! Wie geht's? Mir geht es gut. Hinweis: morgen regnet es\nNeue Zeile"),
        [
            "Hallo!",
            "Wie geht's?",
            "Mir geht es gut.",
            "Hinweis:",
            "morgen regnet es",
            "Neue Zeile"
        ]
    );
}

#[test]
fn a_sentence_is_given_out_as_soon_as_the_next_one_starts() {
    let mut s = Sentences::new();
    assert!(s.push("Das ist der erste Satz").is_empty());
    assert!(s.push(".").is_empty());
    assert!(s.push(" ").is_empty());
    assert_eq!(s.push("Der"), ["Das ist der erste Satz."]);
    assert!(s.push(" zweite").is_empty());
    assert_eq!(s.finish(), ["Der zweite"]);
}

#[test]
fn questions_and_exclamations_go_out_at_the_space() {
    let mut s = Sentences::new();
    assert_eq!(s.push("Wirklich? "), ["Wirklich?"]);
}

#[test]
fn numbers_times_and_urls_are_not_cut() {
    assert_eq!(
        both(
            "Es kostet 3.5 Euro. Um 10:30 Uhr geht es los. Siehe https://example.com/a.b?x=1. Fertig."
        ),
        [
            "Es kostet 3.5 Euro.",
            "Um 10:30 Uhr geht es los.",
            "Siehe https://example.com/a.b?x=1.",
            "Fertig."
        ]
    );
}

#[test]
fn abbreviations_are_not_cut() {
    assert_eq!(
        both("Nimm z. B. den Zug. Bring e.g. a coat. Dr. Weber kommt. Ca. drei Stunden."),
        [
            "Nimm z. B. den Zug.",
            "Bring e.g. a coat.",
            "Dr. Weber kommt.",
            "Ca. drei Stunden."
        ]
    );
}

#[test]
fn dates_and_list_numbers_are_not_cut() {
    assert_eq!(
        both("Am 3. Oktober ist Feiertag. Die Antwort ist 42. Danke."),
        [
            "Am 3. Oktober ist Feiertag.",
            "Die Antwort ist 42.",
            "Danke."
        ]
    );
    assert_eq!(
        both("1. Milch kaufen\n2. Brot holen"),
        ["1. Milch kaufen", "2. Brot holen"]
    );
}

#[test]
fn ellipses_and_quotes_stay_with_their_sentence() {
    assert_eq!(
        both("Moment… Er sagte: „Ja.“ Dann ging er (schnell.) Ende?!"),
        [
            "Moment…",
            "Er sagte:",
            "„Ja.“",
            "Dann ging er (schnell.)",
            "Ende?!"
        ]
    );
}

#[test]
fn table_rows_and_code_lines_are_cut_only_at_line_breaks() {
    assert_eq!(
        both("| Name. | Wert: 1 |\n|---|---|\n```rust\nlet x = a. b; // c? d\n```\nFertig. Ja."),
        [
            "| Name. | Wert: 1 |",
            "|---|---|",
            "```rust",
            "let x = a. b; // c? d",
            "```",
            "Fertig.",
            "Ja."
        ]
    );
    assert_eq!(
        both("````rust\nlet x = a. b;\n```\nstill. code\n````\nDone."),
        [
            "````rust",
            "let x = a. b;",
            "```",
            "still. code",
            "````",
            "Done."
        ]
    );
    assert_eq!(
        both("~~~rust\nlet x = a. b;\n~~~\nDone."),
        ["~~~rust", "let x = a. b;", "~~~", "Done."]
    );
}

#[test]
fn empty_lines_and_trailing_space_give_no_sentences() {
    assert_eq!(both("\n\n  Hallo.  \n\n"), ["Hallo."]);
    assert!(both("   ").is_empty());
}
