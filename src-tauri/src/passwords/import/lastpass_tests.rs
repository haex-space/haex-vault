use std::path::PathBuf;

use super::*;
use crate::error::HaexError;

fn fixture() -> Vec<u8> {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "passwords",
        "lastpass.csv",
    ]
    .iter()
    .collect();
    std::fs::read(path).expect("fixture")
}

fn by_title<'a>(model: &'a ImportModel, title: &str) -> &'a ImportItem {
    model
        .items
        .iter()
        .find(|i| i.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no entry {title}"))
}

fn kv<'a>(item: &'a ImportItem, key: &str) -> Option<&'a str> {
    item.key_values
        .iter()
        .find(|k| k.key == key)
        .and_then(|k| k.value.as_deref())
}

#[test]
fn grouping_is_split_at_slash_and_backslash_into_nested_folders() {
    let model = parse(&fixture()).expect("parse");
    assert_eq!(model.items.len(), 3);
    let names: Vec<&str> = model.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Shopping", "Online", "Servers", "Prod"]);
    let shop = by_title(&model, "Shop");
    let online = model
        .groups
        .iter()
        .find(|g| g.name == "Online")
        .expect("Online");
    assert_eq!(shop.group_ref.as_deref(), Some(online.reference.as_str()));
    assert!(online.parent_ref.is_some());
}

#[test]
fn a_secure_note_has_no_address_and_its_key_lines_become_fields() {
    let model = parse(&fixture()).expect("parse");
    let note = by_title(&model, "Server note");
    assert_eq!(note.url, None);
    assert_eq!(
        note.note.as_deref(),
        Some("NoteType:Server\nHostname:srv.example.invalid\nUsername:root")
    );
    assert_eq!(kv(note, "LastPass: Notiztyp"), Some("Server"));
    assert_eq!(kv(note, "Hostname"), Some("srv.example.invalid"));
    assert_eq!(kv(note, "Username"), Some("root"));
}

#[test]
fn extra_of_a_login_stays_the_note_and_is_not_split() {
    let model = parse(&fixture()).expect("parse");
    let shop = by_title(&model, "Shop");
    assert_eq!(shop.note.as_deref(), Some("plain note\nsecond line"));
    assert!(shop.key_values.is_empty());
    let plain = by_title(&model, "Plain");
    assert_eq!(plain.note.as_deref(), Some("Meeting at 10:30"));
    assert!(plain.key_values.is_empty(), "a time in a note is no field");
}

#[test]
fn favourites_and_totp_are_kept() {
    let model = parse(&fixture()).expect("parse");
    let shop = by_title(&model, "Shop");
    assert!(shop.tags.contains(&"Favorit".to_string()));
    assert_eq!(shop.otp_raw.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert!(!by_title(&model, "Plain")
        .tags
        .contains(&"Favorit".to_string()));
}

#[test]
fn a_file_without_the_lastpass_columns_is_refused() {
    assert!(matches!(
        parse(b"x,y\n1,2\n"),
        Err(HaexError::PasswordsImportFailed { .. })
    ));
}
