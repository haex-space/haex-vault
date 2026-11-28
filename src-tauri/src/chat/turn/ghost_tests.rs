use std::collections::HashSet;

use serde_json::json;

use super::ghost::{ghost, is_ghost, GHOST_START};

fn reads(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_reading_tool_is_named_with_its_arguments_and_may_be_called_again() {
    let input = json!({ "url": "https://a.de" });

    let header = ghost(
        Some(("browser_open", &input)),
        false,
        14_200,
        &reads(&["browser_open"]),
    );

    assert_eq!(
        header,
        format!(
            "{GHOST_START} browser_open({{\"url\":\"https://a.de\"}}) returned a result of 14200 characters. \
             Call it again if you need it.]"
        )
    );
}

#[test]
fn a_changing_tool_is_named_without_the_invitation_to_call_it_again() {
    let input = json!({ "id": "m-17" });

    let header = ghost(
        Some(("mail_delete", &input)),
        false,
        40,
        &reads(&["mail_search"]),
    );

    assert_eq!(
        header,
        format!(
            "{GHOST_START} mail_delete({{\"id\":\"m-17\"}}) returned a result of 40 characters.]"
        )
    );
}

#[test]
fn an_error_is_called_one() {
    let input = json!({});

    let header = ghost(Some(("mail_send", &input)), true, 120, &reads(&[]));

    assert_eq!(
        header,
        format!("{GHOST_START} mail_send() returned an error of 120 characters.]")
    );
}

#[test]
fn long_arguments_are_cut_at_eighty_characters() {
    let input = json!({ "text": "ä".repeat(200) });

    let header = ghost(Some(("note_add", &input)), false, 9, &reads(&[]));

    let shown = header
        .split_once("note_add(")
        .and_then(|(_, rest)| rest.split_once(") returned"))
        .map(|(arguments, _)| arguments)
        .expect("the arguments are shown");
    assert_eq!(shown.chars().count(), 81, "80 characters and the ellipsis");
    assert!(shown.ends_with('…'));
}

#[test]
fn without_its_call_only_the_size_is_known() {
    assert_eq!(
        ghost(None, false, 3_000, &reads(&[])),
        format!("{GHOST_START} it was a result of 3000 characters.]")
    );
}

#[test]
fn only_a_header_is_taken_for_one() {
    let header = ghost(None, false, 1, &reads(&[]));

    assert!(is_ghost(&header));
    assert!(!is_ghost("[Tool result left out to fit the context"));
    assert!(!is_ghost(
        "a page that says [Tool result left out to fit the context: x]"
    ));
}
