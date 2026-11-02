use std::collections::BTreeMap;

use haex_autofill::{EntryKeys, FieldFacts};

use zeroize::Zeroizing;

use super::act::entry_fill::{
    kept_values, keys_of, purpose_of, rows_for, values_of, EntryValues, MAX_PURPOSE,
};
use crate::passwords::model::{SecretItem, SecretKeyValue};

fn field(input_type: &str, name: &str, autocomplete: &str) -> FieldFacts {
    FieldFacts {
        input_type: input_type.to_owned(),
        name: name.to_owned(),
        autocomplete: autocomplete.to_owned(),
        ..FieldFacts::default()
    }
}

/// The fields of an entry with bank and card data and a one-time code.
fn bank_values() -> EntryValues {
    [
        ("IBAN", "DE02120300000000202051"),
        ("Kartennummer", "4111111111111111"),
        ("username", "anna"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), Zeroizing::new(value.to_owned())))
    .collect()
}

/// Their names, as the rows see them: no value.
fn bank() -> EntryKeys {
    EntryKeys {
        keys: bank_values()
            .keys()
            .cloned()
            .chain(["otp".to_owned()])
            .collect(),
        aliases: BTreeMap::new(),
        has_totp: true,
    }
}

#[test]
fn empty_custom_fields_are_not_offered_or_filled() {
    let item = SecretItem {
        id: "entry".to_owned(),
        title: Some("Entry".to_owned()),
        username: None,
        password: None,
        note: None,
        url: None,
        icon: None,
        color: None,
        expires_at: None,
        otp_secret: None,
        otp_digits: None,
        otp_period: None,
        otp_algorithm: None,
        autofill_aliases: None,
        tags: Vec::new(),
        key_values: vec![
            SecretKeyValue {
                id: "empty".to_owned(),
                key: Some("empty field".to_owned()),
                value: Some(String::new()),
            },
            SecretKeyValue {
                id: "filled".to_owned(),
                key: Some("card".to_owned()),
                value: Some("4111111111111111".to_owned()),
            },
        ],
        created_at: None,
        updated_at: None,
    };

    let keys = keys_of(&item, false);
    assert_eq!(keys.keys, vec!["card".to_owned()]);
    let values = values_of(&item);
    assert_eq!(values.len(), 1);
    assert_eq!(values["card"].as_str(), "4111111111111111");
}

fn checkout() -> (Vec<FieldFacts>, Vec<String>) {
    let fields = vec![
        field("text", "iban", ""),
        field("text", "card", "cc-number"),
        field("text", "comment", ""),
        field("text", "code", "one-time-code"),
    ];
    let labels = ["IBAN", "Kartennummer", "Bemerkung", "Bestätigungscode"]
        .map(str::to_owned)
        .to_vec();
    (fields, labels)
}

#[test]
fn the_rows_name_page_fields_and_entry_fields_never_values() {
    let (fields, labels) = checkout();
    let rows = rows_for("e1", 0, None, &fields, &labels, &bank());
    let shown: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|row| (row.id.as_str(), row.field.as_str(), row.key.as_str()))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("e1.0.0", "IBAN", "IBAN"),
            ("e1.0.1", "Kartennummer", "Kartennummer"),
            ("e1.0.3", "Bestätigungscode", "otp"),
        ]
    );
    let text = serde_json::to_string(&rows).unwrap();
    for value in bank_values().values() {
        assert!(!text.contains(value.as_str()), "{text}");
    }
    assert!(rows.iter().all(|row| row.entry == "e1"));
}

#[test]
fn a_deselected_row_stays_empty() {
    let (fields, labels) = checkout();
    let rows = rows_for("e1", 0, None, &fields, &labels, &bank());
    let values = bank_values();
    let kept = kept_values(&rows, &["e1.0.0".to_owned()], &values, Some("123456"));
    assert_eq!(
        kept,
        vec![
            (0, 1, "4111111111111111".to_owned()),
            (0, 3, "123456".to_owned())
        ]
    );
    // Without a code the code row fills nothing.
    let without_code = kept_values(&rows, &[], &values, None);
    assert_eq!(without_code.len(), 2);
}

#[test]
fn a_field_without_a_label_shows_its_name() {
    let (fields, _) = checkout();
    let rows = rows_for("e2", 0, None, &fields, &[], &bank());
    assert_eq!(rows[0].field, "iban");
    assert_eq!(rows[0].id, "e2.0.0");
    assert_eq!(rows[0].frame, None);
}

#[test]
fn the_purpose_is_one_to_two_hundred_characters() {
    assert_eq!(purpose_of("  Lastschrift  "), Some("Lastschrift"));
    assert_eq!(purpose_of("   "), None);
    assert!(purpose_of(&"x".repeat(MAX_PURPOSE)).is_some());
    assert!(purpose_of(&"x".repeat(MAX_PURPOSE + 1)).is_none());
}

#[test]
fn rows_of_a_frame_name_its_website_and_write_into_it() {
    let card = vec![field("text", "pan", "cc-number")];
    let entry = bank();
    let values = bank_values();
    let rows = rows_for(
        "e1",
        2,
        Some("https://pay.example"),
        &card,
        &["Kartennummer".to_owned()],
        &entry,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "e1.2.0");
    assert_eq!(rows[0].frame.as_deref(), Some("https://pay.example"));
    assert_eq!(
        kept_values(&rows, &[], &values, None),
        vec![(2, 0, "4111111111111111".to_owned())]
    );
}
