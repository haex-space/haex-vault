use super::*;

fn luna() -> AssistantName {
    AssistantName::new("Luna", "Luhna").unwrap()
}

#[test]
fn the_default_is_written_haex_and_said_hex() {
    let name = AssistantName::default();
    assert_eq!((name.written(), name.spoken()), ("Haex", "Hex"));
}

#[test]
fn a_name_is_trimmed_and_the_spoken_form_defaults_to_the_written_one() {
    let name = AssistantName::new("  Luna ", "").unwrap();
    assert_eq!((name.written(), name.spoken()), ("Luna", "Luna"));
}

#[test]
fn a_name_has_one_or_two_words_of_letters_digits_and_hyphens() {
    for ok in ["Luna", "Anna-Lena", "R2D2", "Zoë", "Émile", "Anna Lena"] {
        assert!(AssistantName::new(ok, "").is_ok(), "{ok}");
    }
    for bad in ["", "   ", "Eins Zwei Drei", "Lu_na", "Luna!", "Dr. Who"] {
        assert!(AssistantName::new(bad, "").is_err(), "{bad:?}");
    }
    assert_eq!(
        AssistantName::new("Luna\nMond", "").unwrap().written(),
        "Luna Mond",
        "any whitespace is a space"
    );
    assert!(
        AssistantName::new(&"a".repeat(25), "").is_err(),
        "at most 24 characters"
    );
    assert!(AssistantName::new(&"a".repeat(24), "").is_ok());
    assert!(
        AssistantName::new("Luna", "Lu na na").is_err(),
        "the spoken form obeys the same rule"
    );
}

#[test]
fn the_written_form_replaces_the_spoken_one_in_text_that_was_heard() {
    let haex = AssistantName::default();
    assert_eq!(
        haex.heard_in("Hex, wie spät ist es?"),
        "Haex, wie spät ist es?"
    );
    assert_eq!(haex.heard_in("sag hex bitte hallo"), "sag Haex bitte hallo");
    assert_eq!(
        luna().heard_in("luhna, mach Licht an"),
        "Luna, mach Licht an"
    );
}

#[test]
fn only_whole_words_are_replaced() {
    let haex = AssistantName::default();
    assert_eq!(haex.heard_in("Hexen gibt es nicht"), "Hexen gibt es nicht");
    assert_eq!(haex.heard_in("Die Hexe kommt"), "Die Hexe kommt");
    assert_eq!(haex.heard_in("hexadezimal"), "hexadezimal");
}

#[test]
fn a_word_that_is_part_of_a_compound_or_a_code_is_left_alone() {
    // "Hex" is also a word of its own: hex codes and the like are not the assistant.
    let haex = AssistantName::default();
    for text in [
        "Hex-Code",
        "der Hex-Editor",
        "#hex",
        "0xFF Hex-Wert",
        "Farbe als hex-string",
    ] {
        assert_eq!(haex.heard_in(text), text, "{text}");
    }
}

#[test]
fn text_that_already_has_the_written_form_stays() {
    assert_eq!(
        AssistantName::default().heard_in("Haex hat geantwortet"),
        "Haex hat geantwortet"
    );
    // Case is fixed, nothing else changes when both forms are the same.
    let same = AssistantName::new("Max", "Max").unwrap();
    assert_eq!(same.heard_in("max sagt hallo"), "Max sagt hallo");
    assert_eq!(same.spoken_in("Max sagt hallo"), "Max sagt hallo");
}

#[test]
fn the_spoken_form_replaces_the_written_one_before_speaking() {
    let haex = AssistantName::default();
    assert_eq!(
        haex.spoken_in("Haex hat das erledigt."),
        "Hex hat das erledigt."
    );
    assert_eq!(haex.spoken_in("haex, und Haex!"), "Hex, und Hex!");
    assert_eq!(luna().spoken_in("Ich bin Luna."), "Ich bin Luhna.");
    // Not inside other words, and not in the middle of an address or a path.
    assert_eq!(haex.spoken_in("Haextension"), "Haextension");
    assert_eq!(haex.spoken_in("haex-pass"), "haex-pass");
}

#[test]
fn the_prompt_names_the_assistant_in_its_written_form() {
    let line = luna().prompt_line();
    assert!(line.contains("Luna"), "{line}");
    assert!(
        !line.contains("Luhna"),
        "the model never sees the spoken form: {line}"
    );
}

#[test]
fn a_stored_value_that_does_not_parse_is_the_default() {
    assert_eq!(AssistantName::from_stored(None), AssistantName::default());
    assert_eq!(
        AssistantName::from_stored(Some("kein json")),
        AssistantName::default()
    );
    assert_eq!(
        AssistantName::from_stored(Some(r#"{"written":"","spoken":""}"#)),
        AssistantName::default()
    );
    let stored = serde_json::to_string(&luna()).unwrap();
    assert_eq!(AssistantName::from_stored(Some(&stored)), luna());
}

#[test]
fn the_written_form_gets_its_case_back() {
    assert_eq!(
        AssistantName::default().heard_in("haex ist da"),
        "Haex ist da"
    );
}

#[test]
fn code_is_left_alone() {
    let haex = AssistantName::default();
    assert_eq!(
        haex.heard_in("Starte `haex sign` und dann hex"),
        "Starte `haex sign` und dann Haex"
    );
    assert_eq!(
        haex.heard_in("```\nhaex --help\n```\nhex sagt"),
        "```\nhaex --help\n```\nHaex sagt"
    );
}

#[test]
fn a_name_of_two_words_is_found_across_a_space() {
    let name = AssistantName::new("Anna Lena", "Anna Lehna").unwrap();
    assert_eq!(
        name.heard_in("hey anna  lehna, hallo"),
        "hey Anna Lena, hallo"
    );
    assert_eq!(name.spoken_in("Danke, Anna Lena."), "Danke, Anna Lehna.");
}

#[test]
fn written_text_only_gets_the_case_of_the_written_form_fixed() {
    let haex = AssistantName::default();
    assert_eq!(
        haex.spelled_in("haex hat geantwortet"),
        "Haex hat geantwortet"
    );
    // "Hex" is a word of its own in a text someone wrote.
    assert_eq!(
        haex.spelled_in("Die Farbe als Hex: #ff0000"),
        "Die Farbe als Hex: #ff0000"
    );
    assert_eq!(haex.spelled_in("`haex sign` bleibt"), "`haex sign` bleibt");
}
