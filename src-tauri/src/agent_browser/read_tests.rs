//! Tests for cutting and marking a page result (spec 065, T023).

use serde_json::{json, Value};

use super::read::{page_result, Raw, MAX_ELEMENTS, MAX_TEXT_CHARS, NOTE};

fn raw(text: &str, elements: usize) -> Raw {
    Raw {
        title: "Kasse".to_owned(),
        url: "https://shop.example.de/kasse".to_owned(),
        text: text.to_owned(),
        elements: (1..=elements)
            .map(|n| json!({ "n": n, "kind": "link", "name": format!("L{n}"), "visible": true }))
            .collect(),
        gone: false,
    }
}

#[test]
fn a_page_has_the_shape_of_the_contract() {
    let page = page_result("t1", "https://shop.example.de", raw("Hallo", 2), 0);
    assert_eq!(page["tab"], "t1");
    assert_eq!(page["note"], NOTE);
    let content = &page["untrusted_page_content"];
    assert_eq!(content["site"], "https://shop.example.de");
    assert_eq!(content["url"], "https://shop.example.de/kasse");
    assert_eq!(content["title"], "Kasse");
    assert_eq!(content["text"], "Hallo");
    assert_eq!(content["truncated"], false);
    assert_eq!(content["elements"].as_array().unwrap().len(), 2);
    assert!(content.get("next_from").is_none());
    let mut keys: Vec<&String> = page.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["note", "tab", "untrusted_page_content"],
        "nothing from the page outside untrusted_page_content"
    );
}

#[test]
fn long_text_is_cut_and_says_where_to_go_on() {
    let text = "ä".repeat(MAX_TEXT_CHARS + 10);
    let page = page_result("t1", "https://a.de", raw(&text, 0), 0);
    let content = &page["untrusted_page_content"];
    assert_eq!(content["truncated"], true);
    assert_eq!(
        content["text"].as_str().unwrap().chars().count(),
        MAX_TEXT_CHARS
    );
    assert_eq!(content["next_from"], MAX_TEXT_CHARS);

    let rest = page_result("t1", "https://a.de", raw(&text, 0), MAX_TEXT_CHARS);
    let content = &rest["untrusted_page_content"];
    assert_eq!(content["text"].as_str().unwrap().chars().count(), 10);
    assert_eq!(content["truncated"], false);
}

#[test]
fn more_than_300_elements_are_cut() {
    let page = page_result("t1", "https://a.de", raw("x", MAX_ELEMENTS + 1), 0);
    let content = &page["untrusted_page_content"];
    assert_eq!(content["truncated"], true);
    let elements = content["elements"].as_array().unwrap();
    assert_eq!(elements.len(), MAX_ELEMENTS);
    assert_eq!(elements.last().unwrap()["n"], Value::from(MAX_ELEMENTS));
}

#[test]
fn from_beyond_the_end_gives_empty_text() {
    let page = page_result("t1", "https://a.de", raw("kurz", 0), 100);
    assert_eq!(page["untrusted_page_content"]["text"], "");
    assert_eq!(page["untrusted_page_content"]["truncated"], false);
}

#[test]
fn elements_say_whether_a_click_submits_without_the_facts_behind_it() {
    let mut page = raw("Kasse", 0);
    page.elements = vec![
        json!({ "n": 1, "kind": "button", "name": "Jetzt kaufen", "facts": { "tag": "button", "type": "button", "form": false } }),
        json!({ "n": 2, "kind": "button", "name": "Weiter", "facts": { "tag": "button", "type": "", "form": true } }),
        json!({ "n": 3, "kind": "link", "name": "Startseite", "facts": { "tag": "a", "type": "", "form": false } }),
        json!({ "n": 4, "kind": "field", "name": "Bestellnummer", "facts": { "tag": "input", "type": "text", "form": true } }),
    ];
    let result = page_result("t1", "https://shop.example.de", page, 0);
    let elements = result["untrusted_page_content"]["elements"]
        .as_array()
        .unwrap();
    let submits: Vec<bool> = elements
        .iter()
        .map(|element| element.get("submits") == Some(&Value::Bool(true)))
        .collect();
    assert_eq!(submits, [true, true, false, false]);
    assert!(elements
        .iter()
        .all(|element| element.get("facts").is_none()));
}
