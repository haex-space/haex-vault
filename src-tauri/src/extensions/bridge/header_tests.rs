// The checks on an extension's header description (spec 090, contracts/header-description.md #1–#11,
// 9b), at the boundary and one over. Pure: no vault, no bundle.

use serde_json::{json, Value};

use super::header_validate::validate;

fn ok(params: Value) -> Value {
    serde_json::to_value(validate(&params, false).expect("the description is valid")).unwrap()
}

/// The message of the refusal; also checks that it is a `3001`.
fn refused(params: Value) -> String {
    refused_with(params, false)
}

fn refused_with(params: Value, inline: bool) -> String {
    let error = validate(&params, inline).expect_err("the description is refused");
    assert_eq!(error.code.as_u16(), 3001, "{}", error.message);
    error.message
}

fn button(id: &str) -> Value {
    json!({"type": "button", "id": id, "label": "L", "text": "T"})
}

fn buttons(count: usize) -> Value {
    json!({"end": (0..count).map(|i| button(&format!("b{i}"))).collect::<Vec<_>>()})
}

const EXAMPLE: &str = r#"{
  "start": [{"type": "toggle", "id": "sidebar", "label": "Show sidebar", "icon": "icons/sidebar.svg",
             "monochrome": true, "pressed": true, "collapse": {"form": "keep"}}],
  "center": [{"type": "search", "id": "q", "label": "Search notes", "placeholder": "Search…",
              "value": "", "collapse": {"order": 10, "form": "shrink"}}],
  "end": [
    {"type": "choice", "id": "view", "label": "View", "selected": "list",
     "options": [{"id": "list", "label": "List", "icon": "icons/list.svg"},
                 {"id": "grid", "label": "Grid", "icon": "icons/grid.svg"}],
     "collapse": {"order": 20}},
    {"type": "menu", "id": "more", "label": "More", "icon": "icons/more.svg",
     "items": [{"type": "item", "id": "export", "label": "Export…", "icon": "icons/export.svg"},
               {"type": "separator"},
               {"type": "item", "id": "settings", "label": "Settings"}],
     "collapse": {"order": 30, "form": "keep"}}
  ]
}"#;

#[test]
fn the_contract_example_is_valid_and_comes_back_whole() {
    let given: Value = serde_json::from_str(EXAMPLE).unwrap();
    assert_eq!(ok(given.clone()), given);
}

#[test]
fn an_empty_description_clears_the_controls() {
    assert_eq!(ok(json!({})), json!({}));
}

#[test]
fn it_must_be_an_object_whose_areas_are_lists() {
    assert!(refused(json!([])).contains("object"));
    assert!(refused(json!({"start": {}})).contains("start must be a list"));
    assert!(refused(json!({"end": "x"})).contains("end must be a list"));
}

#[test]
fn at_most_32_controls_over_the_three_areas() {
    ok(buttons(32));
    assert!(refused(buttons(33)).contains("32 controls"));
    let split = json!({
        "start": (0..11).map(|i| button(&format!("s{i}"))).collect::<Vec<_>>(),
        "center": (0..11).map(|i| button(&format!("c{i}"))).collect::<Vec<_>>(),
        "end": (0..11).map(|i| button(&format!("e{i}"))).collect::<Vec<_>>(),
    });
    assert!(refused(split).contains("32 controls"));
}

#[test]
fn a_description_is_at_most_32_kib_without_inline_pictures() {
    // Searches take up to 1000 characters of `value` each: fill them up to the byte.
    let build = |target: usize| {
        let mut remaining = target;
        let mut params = json!({"start": (0..32).map(|i| json!({
            "type": "search", "id": format!("s{i}"), "label": "L", "value": ""
        })).collect::<Vec<_>>()});
        remaining -= params.to_string().len();
        for control in params["start"].as_array_mut().unwrap() {
            let take = remaining.min(1000);
            control["value"] = json!("a".repeat(take));
            remaining -= take;
        }
        assert_eq!(remaining, 0, "the target is reachable");
        assert_eq!(params.to_string().len(), target);
        params
    };
    ok(build(32 * 1024));
    assert!(refused(build(32 * 1024 + 1)).contains("32 KiB"));
}

#[test]
fn an_unknown_control_type_is_left_out_and_the_rest_stays() {
    let given = json!({"end": [
        button("a"),
        {"type": "slider", "id": "x", "label": "X"},
        {"type": 7},
        button("b"),
    ]});
    // A control whose `type` is not text is refused; a text the catalog does not know is dropped.
    assert!(refused(given).contains("needs a type"));
    let shown = ok(json!({"end": [
        button("a"),
        {"type": "slider", "id": "x", "label": "X"},
        button("b"),
    ]}));
    let ids: Vec<&str> = shown["end"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn an_element_that_is_not_an_object_is_refused() {
    assert!(refused(json!({"end": ["x"]})).contains("object"));
}

#[test]
fn text_is_1_to_120_characters_and_placeholder_and_tooltip_may_be_empty() {
    let with = |text: &str| json!({"end": [{"type": "button", "id": "a", "label": text, "icon": "i.svg"}]});
    ok(with(&"x".repeat(120)));
    assert!(refused(with(&"x".repeat(121))).contains("label must be 1 to 120"));
    assert!(refused(with("")).contains("label must be 1 to 120"));
    // 120 characters are counted, not bytes.
    ok(with(&"ä".repeat(120)));
    let search = |placeholder: &str, tooltip: &str| {
        json!({"end": [{"type": "search", "id": "q", "label": "L", "value": "",
            "placeholder": placeholder, "tooltip": tooltip}]})
    };
    ok(search("", ""));
    assert!(refused(search(&"p".repeat(121), "")).contains("placeholder"));
    assert!(refused(search("", &"t".repeat(121))).contains("tooltip"));
}

#[test]
fn search_value_and_edit_value_are_up_to_1000_characters() {
    let search =
        |value: &str| json!({"end": [{"type": "search", "id": "q", "label": "L", "value": value}]});
    ok(search(&"v".repeat(1000)));
    assert!(refused(search(&"v".repeat(1001))).contains("value must be 0 to 1000"));
    let crumb = |edit: &str| {
        json!({"end": [{"type": "breadcrumb", "id": "p", "label": "L",
            "segments": [{"id": "a", "label": "A"}], "editable": true, "editValue": edit}]})
    };
    ok(crumb(&"v".repeat(1000)));
    assert!(refused(crumb(&"v".repeat(1001))).contains("editValue"));
}

#[test]
fn an_identifier_is_1_to_64_of_letters_digits_and_dot_underscore_colon_hyphen() {
    let with = |id: &str| json!({"end": [button(id)]});
    ok(with(&"a".repeat(64)));
    ok(with("Aa09._:-"));
    for bad in [&"a".repeat(65)[..], "", "a b", "a/b", "ä", "a;"] {
        assert!(refused(with(bad)).contains("id must be"), "{bad:?}");
    }
    assert!(
        refused(json!({"end": [{"type": "button", "label": "L", "text": "T"}]}))
            .contains("id must be")
    );
}

#[test]
fn an_identifier_is_unique_over_controls_entries_options_and_segments() {
    assert!(refused(json!({"start": [button("a")], "end": [button("a")]})).contains("used twice"));
    let menu = |item: &str| {
        json!({"end": [button("a"), {"type": "menu", "id": "m", "label": "M", "icon": "i.svg",
            "items": [{"type": "item", "id": item, "label": "I"}]}]})
    };
    ok(menu("x"));
    assert!(refused(menu("a")).contains("used twice"));
    assert!(refused(menu("m")).contains("used twice"));
    let choice = json!({"end": [button("o"), {"type": "choice", "id": "c", "label": "C", "selected": "o",
        "options": [{"id": "o", "label": "O"}]}]});
    assert!(refused(choice).contains("used twice"));
    let crumb = json!({"end": [button("s"), {"type": "breadcrumb", "id": "p", "label": "P",
        "segments": [{"id": "s", "label": "S"}]}]});
    assert!(refused(crumb).contains("used twice"));
}

#[test]
fn a_menu_has_up_to_64_entries() {
    let menu = |count: usize| {
        json!({"end": [{"type": "menu", "id": "m", "label": "M", "icon": "i.svg",
            "items": (0..count).map(|i| json!({"type": "item", "id": format!("i{i}"), "label": "I"})).collect::<Vec<_>>()}]})
    };
    ok(menu(64));
    assert!(refused(menu(65)).contains("items must be a list"));
    ok(menu(0));
    assert!(refused(
        json!({"end": [{"type": "menu", "id": "m", "label": "M", "icon": "i.svg",
        "items": [{"type": "link", "id": "x", "label": "X"}]}]})
    )
    .contains("item or a separator"));
}

#[test]
fn a_menu_entry_can_be_checked() {
    let menu = |checked: Value| {
        json!({"end": [{"type": "menu", "id": "m", "label": "M", "icon": "i.svg",
            "items": [{"type": "item", "id": "a", "label": "A", "checked": checked}]}]})
    };
    let kept = ok(menu(json!(true)));
    assert_eq!(kept["end"][0]["items"][0]["checked"], json!(true));
    assert!(refused(menu(json!("yes"))).contains("checked"));
}

#[test]
fn a_choice_has_1_to_8_options_and_selects_one_of_them() {
    let choice = |count: usize, selected: &str| {
        json!({"end": [{"type": "choice", "id": "c", "label": "C", "selected": selected,
            "options": (0..count).map(|i| json!({"id": format!("o{i}"), "label": "O"})).collect::<Vec<_>>()}]})
    };
    ok(choice(8, "o7"));
    assert!(refused(choice(9, "o0")).contains("options must be a list"));
    assert!(refused(choice(0, "o0")).contains("options must be a list"));
    assert!(refused(choice(2, "zzz")).contains("selected must be the id of an option"));
}

#[test]
fn a_breadcrumb_has_1_to_64_segments() {
    let crumb = |count: usize| {
        json!({"end": [{"type": "breadcrumb", "id": "p", "label": "P",
            "segments": (0..count).map(|i| json!({"id": format!("s{i}"), "label": "S"})).collect::<Vec<_>>()}]})
    };
    ok(crumb(64));
    assert!(refused(crumb(65)).contains("segments must be a list"));
    assert!(refused(crumb(0)).contains("segments must be a list"));
}

#[test]
fn a_label_is_needed_except_for_a_title_and_a_separator() {
    for kind in ["button", "toggle", "search", "menu", "choice", "breadcrumb"] {
        let control = json!({"type": kind, "id": "a", "text": "T", "icon": "i.svg", "pressed": true,
            "value": "", "items": [], "options": [{"id": "o", "label": "O"}], "selected": "o",
            "segments": [{"id": "s", "label": "S"}]});
        assert!(
            refused(json!({"end": [control]})).contains("label is needed"),
            "{kind}"
        );
    }
    ok(
        json!({"end": [{"type": "title", "id": "t", "text": "Notes"}, {"type": "separator", "id": "s"}]}),
    );
}

#[test]
fn a_button_toggle_or_menu_has_an_icon_or_a_text() {
    assert!(
        refused(json!({"end": [{"type": "button", "id": "a", "label": "L"}]}))
            .contains("icon or a text")
    );
    assert!(refused(
        json!({"end": [{"type": "toggle", "id": "a", "label": "L", "pressed": false}]})
    )
    .contains("icon or a text"));
    ok(json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": "i.svg"}]}));
    ok(
        json!({"end": [{"type": "toggle", "id": "a", "label": "L", "pressed": false, "text": "T"}]}),
    );
}

#[test]
fn fields_must_have_the_type_of_the_catalog() {
    let one = |extra: Value| {
        let mut control =
            json!({"type": "toggle", "id": "a", "label": "L", "text": "T", "pressed": true});
        control
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"end": [control]})
    };
    assert!(refused(one(json!({"pressed": "yes"}))).contains("pressed"));
    assert!(refused(one(json!({"disabled": 1}))).contains("disabled"));
    assert!(refused(one(json!({"monochrome": "x"}))).contains("monochrome"));
    assert!(refused(one(json!({"label": 7}))).contains("label must be text"));
}

#[test]
fn collapse_order_is_a_whole_number_0_to_1000_and_form_one_of_four() {
    let with = |collapse: Value| json!({"end": [{"type": "button", "id": "a", "label": "L", "text": "T", "collapse": collapse}]});
    ok(with(json!({"order": 0})));
    ok(with(json!({"order": 1000})));
    assert!(refused(with(json!({"order": 1001}))).contains("collapse.order"));
    assert!(refused(with(json!({"order": -1}))).contains("collapse.order"));
    assert!(refused(with(json!({"order": 1.5}))).contains("collapse.order"));
    for form in ["overflow", "icon", "shrink", "keep"] {
        ok(with(json!({"form": form})));
    }
    assert!(refused(with(json!({"form": "hide"}))).contains("collapse.form"));
    assert!(refused(with(json!("keep"))).contains("collapse must be an object"));
}

#[test]
fn a_test_id_is_for_holzis_own_apps() {
    let message = refused(
        json!({"end": [{"type": "button", "id": "a", "label": "L", "text": "T", "testId": "x"}]}),
    );
    assert!(message.contains("testId"));
}

#[test]
fn texts_hold_no_control_or_bidirectional_formatting_character() {
    for bad in [
        "a\u{0}b",
        "a\nb",
        "a\u{7f}b",
        "a\u{85}b",
        "\u{202e}evil",
        "a\u{2066}b",
        "a\u{200f}b",
    ] {
        let message =
            refused(json!({"end": [{"type": "button", "id": "a", "label": bad, "text": "T"}]}));
        assert!(message.contains("control character"), "{bad:?}");
    }
    let message =
        refused(json!({"end": [{"type": "search", "id": "q", "label": "L", "value": "a\u{1b}b"}]}));
    assert!(message.contains("value holds a control character"));
}

#[test]
fn a_text_next_to_an_icon_is_checked_like_any_other() {
    for kind in ["button", "menu"] {
        let mut control = json!({"type": kind, "id": "a", "label": "L", "icon": "icon.svg",
            "text": "\u{202e}evil"});
        if kind == "menu" {
            control["items"] = json!([]);
        }
        let message = refused(json!({"end": [control.clone()]}));
        assert!(message.contains("text holds a control character"), "{kind}");
        control["text"] = json!("x".repeat(121));
        let message = refused(json!({"end": [control]}));
        assert!(
            message.contains("text must be 1 to 120 characters"),
            "{kind}"
        );
    }
    let toggle = json!({"type": "toggle", "id": "a", "label": "L", "icon": "icon.svg",
        "pressed": false, "text": "a\nb"});
    assert!(refused(json!({"end": [toggle]})).contains("text holds a control character"));
}

#[test]
fn a_refusal_names_the_control_and_the_field_and_never_quotes_the_text() {
    let message = refused(
        json!({"end": [{"type": "button", "id": "save", "label": "SECRET\u{202e}TEXT", "text": "T"}]}),
    );
    assert_eq!(
        message,
        "header: control \"save\": label holds a control character"
    );
    assert!(!message.contains("SECRET"));
}

#[test]
fn a_label_with_markup_is_just_that_text() {
    let label = "<img src=x onerror=alert(1)>";
    let shown = ok(json!({"end": [{"type": "button", "id": "a", "label": label, "text": label}]}));
    assert_eq!(shown["end"][0]["label"], label);
    assert_eq!(shown["end"][0]["text"], label);
}

#[test]
fn icons_are_files_of_the_bundle_in_one_of_five_picture_types() {
    let with =
        |icon: &str| json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": icon}]});
    for good in [
        "icons/a.svg",
        "a.png",
        "x/y/a.JPG",
        "a.jpeg",
        "a.webp",
        "a.gif",
        "./icons/a.svg",
        "/icons/a.svg",
    ] {
        ok(with(good));
    }
    for bad in [
        "lucide:plus",
        "https://example.org/a.png",
        "//example.org/a.png",
        "../a.png",
        "icons/../../a.png",
        "icons\\a.png",
        "icons/a.html",
        "icons/a.ico",
        "icons/noextension",
        "",
        "C:/a.png",
    ] {
        let message = refused(with(bad));
        assert!(message.contains("control \"a\""), "{bad:?}: {message}");
        assert!(message.contains("icon"), "{bad:?}: {message}");
    }
    let long = format!("{}.png", "a".repeat(300));
    assert!(refused(with(&long)).contains("icon"));
    assert!(
        refused(json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": 7}]}))
            .contains("icon must be text")
    );
}

#[test]
fn menu_entries_and_options_have_icons_by_the_same_rules() {
    let menu = json!({"end": [{"type": "menu", "id": "m", "label": "M", "text": "T",
        "items": [{"type": "item", "id": "i", "label": "I", "icon": "lucide:plus"}]}]});
    assert!(refused(menu).contains("icon"));
    let choice = json!({"end": [{"type": "choice", "id": "c", "label": "C", "selected": "o",
        "options": [{"id": "o", "label": "O", "icon": "https://x/y.png"}]}]});
    assert!(refused(choice).contains("icon"));
}

#[test]
fn an_inline_picture_is_for_development_versions_only() {
    let params = json!({"end": [{"type": "button", "id": "a", "label": "L",
        "icon": "data:image/svg+xml;base64,PHN2Zy8+"}]});
    assert!(refused(params.clone()).contains("only for development versions"));
    assert!(validate(&params, true).is_ok());
    // Inline text counts only up to twice the picture limit; the description limit leaves it out.
    let huge = format!("data:image/png;base64,{}", "A".repeat(2 * 256 * 1024));
    let params = json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": huge}]});
    assert!(refused_with(params, true).contains("256 KiB"));
    let fits = format!("data:image/png;base64,{}", "A".repeat(2 * 256 * 1024 - 22));
    assert!(validate(
        &json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": fits}]}),
        true
    )
    .is_ok());
}

#[test]
fn a_table_of_icons_sent_by_the_extension_is_dropped() {
    let shown = ok(json!({"end": [button("a")], "icons": {"x.png": "data:image/png;base64,AAAA"}}));
    assert!(shown.get("icons").is_none());
}

#[test]
fn fields_the_catalog_does_not_know_do_not_reach_the_frontend() {
    let shown = ok(
        json!({"end": [{"type": "button", "id": "a", "label": "L", "text": "T",
        "onclick": "alert(1)", "style": "position:fixed"}], "extra": 1}),
    );
    assert_eq!(shown, json!({"end": [button("a")]}));
}
