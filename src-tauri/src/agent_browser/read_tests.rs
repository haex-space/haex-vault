//! Tests for cutting and marking a page result (spec 065, T023).

use serde_json::{json, Value};

use super::read::{
    limits, page_result, page_result_within, Raw, MAX_ELEMENTS, MAX_TEXT_CHARS, NOTE,
};

fn raw(text: &str, elements: usize) -> Raw {
    Raw {
        title: "Kasse".to_owned(),
        url: "https://shop.example.de/kasse".to_owned(),
        text: text.to_owned(),
        elements: (1..=elements)
            .map(|n| json!({ "n": n, "kind": "link", "name": format!("L{n}"), "visible": true }))
            .collect(),
        total: elements,
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

#[test]
fn a_smaller_budget_cuts_text_and_elements_and_says_so() {
    let text = "a".repeat(5_000);
    let page = page_result_within("t1", "https://a.de", raw(&text, 100), 0, 2_000, 30);
    let content = &page["untrusted_page_content"];
    assert_eq!(content["text"].as_str().unwrap().chars().count(), 2_000);
    assert_eq!(content["next_from"], 2_000);
    assert_eq!(content["elements"].as_array().unwrap().len(), 30);
    assert_eq!(content["truncated"], true);
}

#[test]
fn elements_in_view_come_first_when_not_all_fit_and_keep_their_numbers() {
    let mut page = raw("x", 0);
    page.elements = (1..=10)
        .map(|n| json!({ "n": n, "kind": "link", "name": format!("L{n}"), "visible": n >= 8 }))
        .collect();
    let result = page_result_within("t1", "https://a.de", page, 0, 1_000, 5);
    let numbers: Vec<u64> = result["untrusted_page_content"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|element| element["n"].as_u64().unwrap())
        .collect();
    // The three in view, then the first two of the rest, shown in the order of the page.
    assert_eq!(numbers, [1, 2, 8, 9, 10]);
}

#[tokio::test]
async fn the_limits_follow_the_budget_of_a_small_context_and_never_go_below_a_usable_page() {
    assert_eq!(limits(), (MAX_TEXT_CHARS, MAX_ELEMENTS));
    let small = crate::chat::tools::context_budget::within(Some(8192), async { limits() }).await;
    assert!(
        small.0 < MAX_TEXT_CHARS && small.1 < MAX_ELEMENTS,
        "{small:?}"
    );
    assert!(small.0 >= 1_500 && small.1 >= 20, "{small:?}");
    let tiny = crate::chat::tools::context_budget::within(Some(1024), async { limits() }).await;
    assert_eq!(tiny, (1_500, 20), "a floor, however small the context");
    let large =
        crate::chat::tools::context_budget::within(Some(1_000_000), async { limits() }).await;
    assert_eq!(large, (MAX_TEXT_CHARS, MAX_ELEMENTS));
}

#[test]
fn canvas_media_and_custom_elements_keep_their_box_and_state() {
    let mut page = raw("Spiel", 0);
    page.elements = vec![
        json!({ "n": 1, "kind": "canvas", "name": "Spielfeld", "visible": true, "box": { "x": 40, "y": 120, "w": 640, "h": 400 }, "facts": { "tag": "canvas", "type": "", "form": false } }),
        json!({ "n": 2, "kind": "media", "name": "Intro", "visible": true, "paused": true, "time": 0.0, "duration": 93.4, "muted": false, "box": { "x": 40, "y": 540, "w": 640, "h": 360 }, "facts": { "tag": "video", "type": "", "form": false } }),
        json!({ "n": 3, "kind": "custom", "name": "Mehr anzeigen", "visible": true, "facts": { "tag": "div", "type": "", "form": false } }),
    ];
    let result = page_result("t1", "https://a.de", page, 0);
    let elements = result["untrusted_page_content"]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(elements[0]["box"]["w"], 640);
    assert_eq!(elements[1]["duration"], 93.4);
    assert_eq!(elements[1]["paused"], true);
    assert_eq!(elements[2]["kind"], "custom");
    assert!(elements
        .iter()
        .all(|element| element.get("submits").is_none()));
}

#[test]
fn a_custom_widget_is_judged_by_its_name_like_any_other_element() {
    let mut page = raw("Shop", 0);
    page.elements = vec![
        json!({ "n": 1, "kind": "custom", "name": "Jetzt kaufen", "facts": { "tag": "div", "type": "", "form": false } }),
        json!({ "n": 2, "kind": "custom", "name": "Mehr anzeigen", "facts": { "tag": "div", "type": "", "form": false } }),
    ];
    let result = page_result("t1", "https://a.de", page, 0);
    let elements = result["untrusted_page_content"]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(elements[0]["submits"], true, "a widget that buys may ask");
    assert!(elements[1].get("submits").is_none());
}
