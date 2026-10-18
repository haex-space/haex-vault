//! Tests for the guard over a model's output (spec 055 FR-010, research R10).

use serde_json::json;

use super::guard::{check, Unusable};

fn schema() -> serde_json::Value {
    json!({
        "type": "object", "required": ["texts"],
        "properties": { "texts": { "type": "array", "items": { "type": "string" } } }
    })
}

const CONTEXT: &str = "Korrigiere nur Rechtschreibung. Gib nur JSON aus.";

fn texts(list: &[&str]) -> String {
    json!({ "texts": list }).to_string()
}

#[test]
fn a_valid_answer_is_returned_parsed() {
    let value = check(&texts(&["Hallo Welt", "zweite Zeile"]), CONTEXT, &schema()).expect("valid");
    assert_eq!(value["texts"][1], "zweite Zeile");
}

#[test]
fn a_code_fence_is_taken_off() {
    let fenced = format!("```json\n{}\n```", texts(&["ok"]));
    assert!(check(&fenced, CONTEXT, &schema()).is_ok());
    let bare = format!("```\n{}\n```", texts(&["ok"]));
    assert!(check(&bare, CONTEXT, &schema()).is_ok());
    // Small models put a sentence before or after the fence, or write it on one line.
    let chatty = format!(
        "Hier ist das JSON:\n```json\n{}\n```\nViel Erfolg!",
        texts(&["ok"])
    );
    assert!(check(&chatty, CONTEXT, &schema()).is_ok());
    let one_line = format!("```{}```", texts(&["ok"]));
    assert!(check(&one_line, CONTEXT, &schema()).is_ok());
    // A fence inside a string of valid JSON is content, not a wrapper.
    let quoting = texts(&["Code: ```x```"]);
    assert_eq!(
        check(&quoting, CONTEXT, &schema()).expect("valid")["texts"][0],
        "Code: ```x```"
    );
}

#[test]
fn nothing_and_blanks_are_empty() {
    for output in ["", "   ", "\n\t\n", "```json\n```"] {
        assert_eq!(
            check(output, CONTEXT, &schema()),
            Err(Unusable::Empty),
            "{output:?}"
        );
    }
}

#[test]
fn the_loop_of_the_measurement_is_found_in_a_closed_answer_and_in_a_cut_one() {
    // What Qwen3-VL-4B wrote on the unreadable poster: "1.00 Uhr," over and over.
    let looping = "1.00 Uhr, ".repeat(120);
    let closed = texts(&[&format!("Morgens: 7.00 Uhr, {looping}")]);
    assert_eq!(check(&closed, CONTEXT, &schema()), Err(Unusable::Loop));

    let cut = format!("{{\"texts\": [\"Morgens: 7.00 Uhr, {looping}");
    assert_eq!(check(&cut, CONTEXT, &schema()), Err(Unusable::Loop));
}

#[test]
fn a_run_of_words_and_a_line_repeated_back_to_back_are_loops() {
    let phrase = "und so weiter ".repeat(8);
    assert_eq!(
        check(&texts(&[&phrase]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
    let line = "Das ist eine Zeile\n".repeat(3);
    assert_eq!(
        check(&texts(&[&line]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
    let same_word = "Uhr ".repeat(40);
    assert_eq!(
        check(&texts(&[&same_word]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
}

#[test]
fn a_low_share_of_distinct_words_is_a_loop() {
    // 100 words from a cycle of nine: no run of eight or fewer words repeats, but only nine differ.
    let cycle = [
        "eins", "zwei", "drei", "vier", "fuenf", "sechs", "sieben", "acht", "neun",
    ];
    let words: Vec<&str> = (0..100).map(|n| cycle[n % cycle.len()]).collect();
    assert_eq!(
        check(&texts(&[&words.join(" ")]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
}

#[test]
fn repetitive_but_honest_answers_are_not_loops() {
    // A yes/no table: many identical short cells.
    let cells: Vec<&str> = std::iter::repeat_n("ja", 20).collect();
    assert!(check(&texts(&cells), CONTEXT, &schema()).is_ok());
    // A page of 150 regions repeats its keys but not its content.
    let regions: Vec<_> = (0..150)
        .map(|n| json!({"kind": "text", "box": [n, n + 1, n + 2, n + 3], "text": format!("Zeile {n} mit etwas Text")}))
        .collect();
    let page_schema = json!({"type": "object", "properties": {"regions": {"type": "array"}}});
    let page = json!({ "regions": regions }).to_string();
    assert!(check(&page, CONTEXT, &page_schema).is_ok());
}

#[test]
fn a_switch_to_another_script_is_dropped() {
    // What PaddleOCR-VL wrote on the poster: Chinese characters in German notes.
    let mixed = texts(&["Staubblätter 详见数据库 Kelchblätter 翰尔"]);
    assert_eq!(
        check(&mixed, CONTEXT, &schema()),
        Err(Unusable::ScriptSwitch)
    );
    let cyrillic = texts(&["Привет мир, это не немецкий текст"]);
    assert_eq!(
        check(&cyrillic, CONTEXT, &schema()),
        Err(Unusable::ScriptSwitch)
    );
}

#[test]
fn a_script_of_the_context_and_a_few_symbols_are_fine() {
    let greek_context = "Schreibe auch griechische Wörter wie λόγος oder φύσις.";
    let greek = texts(&["λόγος und φύσις sind Wörter"]);
    assert!(check(&greek, greek_context, &schema()).is_ok());
    // A physics note: two Greek letters among a lot of German.
    let physics =
        texts(&["Die Wellenlänge λ hängt von der Frequenz ab, und α ist der Winkel dazu"]);
    assert!(check(&physics, CONTEXT, &schema()).is_ok());
    // Digits, umlauts and punctuation never count as foreign.
    assert!(check(&texts(&["Größe: 12,5 cm – äöüß ✓"]), CONTEXT, &schema()).is_ok());
}

#[test]
fn text_that_is_not_json_is_invalid_json() {
    assert_eq!(
        check("Das ist kein JSON.", CONTEXT, &schema()),
        Err(Unusable::InvalidJson)
    );
    assert_eq!(
        check("{\"texts\": [\"abgeschnitten", CONTEXT, &schema()),
        Err(Unusable::InvalidJson)
    );
}

#[test]
fn a_well_formed_answer_against_the_format_is_judged_by_the_schema() {
    let error = check("{\"texts\": [1]}", CONTEXT, &schema()).unwrap_err();
    assert!(
        matches!(&error, Unusable::Schema(violation) if violation.contains("/texts/0")),
        "{error:?}"
    );
    let missing = check("{}", CONTEXT, &schema()).unwrap_err();
    assert!(matches!(&missing, Unusable::Schema(violation) if violation.contains("`texts`")));
}

#[test]
fn invented_but_well_formed_text_passes_on_purpose() {
    // The guard drops the obviously broken; it cannot tell invented sentences from read ones.
    let invented = texts(&["The following table shows the results of the test results."]);
    assert!(check(&invented, CONTEXT, &schema()).is_ok());
}

#[test]
fn every_reason_has_a_short_text() {
    assert_eq!(Unusable::Empty.reason(), "empty");
    assert_eq!(Unusable::Loop.reason(), "loop");
    assert_eq!(Unusable::ScriptSwitch.reason(), "script switch");
    assert_eq!(Unusable::InvalidJson.reason(), "invalid json");
    assert_eq!(
        Unusable::Schema("/a: nope".into()).reason(),
        "schema: /a: nope"
    );
}

#[test]
fn equal_cells_and_answer_lines_of_a_worksheet_are_not_loops() {
    // A timetable row: maths three days running in the same period.
    let row = texts(&["1", "Mathematik", "Mathematik", "Mathematik", "Englisch"]);
    assert!(check(&row, CONTEXT, &schema()).is_ok());
    // Answer lines to fill in, drawn with underscores or dots.
    let lines = texts(&["Antwort:\n________________\n________________\n________________"]);
    assert!(check(&lines, CONTEXT, &schema()).is_ok());
    let dots = texts(&["................\n................\n................"]);
    assert!(check(&dots, CONTEXT, &schema()).is_ok());
}

#[test]
fn the_loop_thresholds_hold_at_their_edges() {
    let pair = |times| "rote Rose ".repeat(times);
    assert!(check(&texts(&[&pair(5)]), CONTEXT, &schema()).is_ok());
    assert_eq!(
        check(&texts(&[&pair(6)]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
    let word = |times| "ja ".repeat(times);
    assert!(check(&texts(&[&word(29)]), CONTEXT, &schema()).is_ok());
    assert_eq!(
        check(&texts(&[&word(30)]), CONTEXT, &schema()),
        Err(Unusable::Loop)
    );
    let line = |times| "Das ist eine Zeile\n".repeat(times);
    assert!(check(&texts(&[&line(2)]), CONTEXT, &schema()).is_ok());
}

#[test]
fn single_greek_letters_of_a_formula_are_no_switch() {
    let angles = texts(&["α = 30°", "β = 60°", "γ = 90°"]);
    assert!(check(&angles, CONTEXT, &schema()).is_ok());
    let triangle = texts(&["a b c α β γ", "Δx = 2 cm"]);
    assert!(check(&triangle, CONTEXT, &schema()).is_ok());
}

#[test]
fn the_script_share_holds_at_its_edge() {
    // Three foreign letters against twelve Latin ones are a fifth; one Latin letter more is not.
    assert_eq!(
        check(&texts(&["мир abcdefghijkl"]), CONTEXT, &schema()),
        Err(Unusable::ScriptSwitch)
    );
    assert!(check(&texts(&["мир abcdefghijklm"]), CONTEXT, &schema()).is_ok());
}
