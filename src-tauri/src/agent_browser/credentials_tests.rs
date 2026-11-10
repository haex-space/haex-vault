use haex_autofill::{EntryKeys, FieldFacts};
use serde_json::json;

use super::act::credentials::{password_caller, values_for, LoginSecrets, FEATURE};
use super::executor::hide_filled;
use super::prompts::{BrowserPromptKind, BrowserQuestion};
use super::redaction::{spellings, Filled};
use super::submit::BrowserField;
use crate::caller::Caller;

const SECRET: &str = "p@ss w&rd<1>";

fn filled() -> Filled {
    let mut filled = Filled::default();
    filled.add(SECRET);
    filled
}

#[test]
fn every_spelling_of_a_filled_value_is_removed() {
    let filled = filled();
    for spelling in spellings(SECRET) {
        let text = format!("before {spelling} after");
        assert_eq!(filled.scrub(&text), "before ••• after", "{spelling}");
    }
    let some: Vec<String> = spellings(SECRET);
    for expected in [
        "p%40ss+w%26rd%3C1%3E",
        "p%40ss%20w%26rd%3C1%3E",
        "cEBzcyB3JnJkPDE+",
        "p@ss w&amp;rd&lt;1&gt;",
    ] {
        assert!(some.iter().any(|s| s == expected), "{expected}");
    }
}

#[test]
fn replies_lose_filled_values_everywhere_in_them() {
    let filled = filled();
    let reply = json!({
        "untrusted_page_content": {
            "text": format!("Willkommen, {SECRET}!"),
            "elements": [{ "name": "p%40ss+w%26rd%3C1%3E", "value": 3 }],
            SECRET: "key"
        }
    });

    let shown = filled.scrub_json(&reply);

    assert!(!shown.to_string().contains("p@ss"));
    assert!(!shown.to_string().contains("p%40ss"));
    assert_eq!(shown["untrusted_page_content"]["text"], "Willkommen, •••!");
    assert_eq!(shown["untrusted_page_content"]["elements"][0]["value"], 3);
}

#[test]
fn any_fill_counts_even_when_the_value_is_too_short_to_search() {
    let mut filled = Filled::default();
    assert!(!filled.any());

    filled.add("ab");

    assert!(filled.any(), "screenshots stay refused");
    assert_eq!(filled.scrub("ab cd"), "ab cd");
}

#[test]
fn questions_show_filled_values_as_dots() {
    let mut question = BrowserQuestion {
        kind: BrowserPromptKind::Submit,
        agent: String::new(),
        site: None,
        tool: None,
        label: Some(format!("POST https://example.org/?p={SECRET}")),
        fields: vec![
            BrowserField {
                name: "pw".to_owned(),
                value: SECRET.to_owned(),
            },
            BrowserField {
                name: "menge".to_owned(),
                value: "2".to_owned(),
            },
        ],
        reason: None,
        bytes: None,
        entries: Vec::new(),
        profile: None,
        mapping: Vec::new(),
    };

    hide_filled(&filled(), &mut question);

    assert_eq!(question.fields[0].value, "•••");
    assert_eq!(question.fields[1].value, "2");
    assert!(!question.label.unwrap().contains("p@ss"));
}

fn facts(input_type: &str, name: &str, autocomplete: &str) -> FieldFacts {
    FieldFacts {
        input_type: input_type.into(),
        name: name.into(),
        autocomplete: autocomplete.into(),
        ..FieldFacts::default()
    }
}

fn login(has_totp: bool) -> LoginSecrets {
    let values: std::collections::BTreeMap<String, zeroize::Zeroizing<String>> =
        [("username", "anna@example.org"), ("password", SECRET)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), zeroize::Zeroizing::new(v.to_owned())))
            .collect();
    LoginSecrets {
        keys: EntryKeys {
            keys: values.keys().cloned().collect(),
            has_totp,
            ..EntryKeys::default()
        },
        values,
    }
}

#[test]
fn a_login_form_gets_username_password_and_the_current_code() {
    let fields = [
        facts("email", "email", ""),
        facts("password", "passwd", ""),
        facts("text", "code", "one-time-code"),
        facts("text", "comment", ""),
    ];

    assert_eq!(
        values_for(&fields, &login(true), Some("123456")),
        vec![
            (0, "anna@example.org".to_owned()),
            (1, SECRET.to_owned()),
            (2, "123456".to_owned()),
        ]
    );
}

#[test]
fn the_totp_marker_is_never_typed() {
    let fields = [facts("text", "otp", "")];

    assert_eq!(values_for(&fields, &login(true), None), Vec::new());
}

#[test]
fn custom_entry_fields_and_card_fields_are_never_filled_by_login_fill() {
    let mut entry = login(true);
    entry.keys.keys.push("iban".to_owned());
    entry.values.insert(
        "iban".to_owned(),
        zeroize::Zeroizing::new("DE89370400440532013000".to_owned()),
    );
    let fields = [facts("text", "iban", ""), facts("text", "", "cc-number")];

    assert!(values_for(&fields, &entry, Some("123456")).is_empty());
}

#[test]
fn the_built_in_agent_reads_through_the_holzi_function() {
    assert_eq!(
        password_caller(&Caller::BuiltinAgent),
        Caller::Internal { feature: FEATURE }
    );
    let external = Caller::ExternalAgent {
        id: "a1".to_owned(),
    };
    assert_eq!(password_caller(&external), external);
}
