use std::path::PathBuf;

use super::*;
use crate::error::HaexError;
use crate::passwords::model::AttentionKind;

const MARKER: &str = "SECRET-MARKER-IMPORT";

fn fixture(name: &str) -> Vec<u8> {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "passwords",
        name,
    ]
    .iter()
    .collect();
    std::fs::read(path).expect("fixture")
}

fn kv<'a>(item: &'a ImportItem, key: &str) -> Option<&'a str> {
    item.key_values
        .iter()
        .find(|k| k.key == key)
        .and_then(|k| k.value.as_deref())
}

fn by_title<'a>(model: &'a ImportModel, title: &str) -> &'a ImportItem {
    model
        .items
        .iter()
        .find(|i| i.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no entry {title}"))
}

#[test]
fn the_json_export_gives_every_entry_and_nested_folders() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    assert_eq!(model.items.len(), 9);
    let names: Vec<(&str, Option<&str>)> = model
        .groups
        .iter()
        .map(|g| (g.name.as_str(), g.parent_ref.as_deref()))
        .collect();
    assert_eq!(names.len(), 2, "Work and Work/Email");
    assert!(names.iter().any(|(n, p)| *n == "Work" && p.is_none()));
    assert!(names.iter().any(|(n, p)| *n == "Email" && p.is_some()));
}

#[test]
fn a_login_keeps_everything_it_has() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    let mail = by_title(&model, "Mail");
    assert_eq!(mail.username.as_deref(), Some("alice@example.invalid"));
    assert_eq!(mail.url.as_deref(), Some("https://mail.example.invalid"));
    assert_eq!(mail.note.as_deref(), Some("line one\nline two"));
    assert_eq!(mail.otp_raw.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(mail.created_at.as_deref(), Some("2024-01-05T08:30:00.000Z"));
    assert_eq!(mail.updated_at.as_deref(), Some("2024-03-02T10:00:00.000Z"));
    assert_eq!(kv(mail, "URL 2"), Some("https://alt.example.invalid"));
    assert_eq!(kv(mail, "URL 2 Zuordnung"), Some("1"));
    assert_eq!(kv(mail, "URL 3"), Some("https://third.example.invalid"));
    assert_eq!(kv(mail, "Security question"), Some("first pet"));
    assert_eq!(kv(mail, "Newsletter"), Some("true"));
    assert_eq!(kv(mail, "Linked user"), Some("Verknüpft: 100"));
    assert_eq!(kv(mail, "Bitwarden: Passwort erneut abfragen"), Some("ja"));
    assert!(kv(mail, "Bitwarden: passwordRevisionDate").is_some());
    assert!(mail.tags.contains(&"Favorit".to_string()));
    assert!(mail.group_ref.is_some() && !mail.trashed);
    assert!(mail.problems.is_empty());
}

#[test]
fn password_history_becomes_states_with_the_source_times() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    let mail = by_title(&model, "Mail");
    assert_eq!(mail.history.len(), 2);
    assert_eq!(
        mail.history[0].modified_at.as_deref(),
        Some("2023-06-01T09:00:00.000Z")
    );
    assert_eq!(
        mail.history[0].data.password.as_deref(),
        Some(format!("{MARKER}-old1").as_str())
    );
    assert_eq!(
        mail.history[1].data.username.as_deref(),
        Some("alice@example.invalid")
    );
}

#[test]
fn cards_identities_ssh_keys_and_unknown_types_lose_nothing() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    let card = by_title(&model, "Visa");
    assert!(card.tags.contains(&"credit-card".to_string()));
    for (key, value) in [
        ("Karteninhaber", "A. Example"),
        ("Marke", "Visa"),
        ("Kartennummer", "4111111111111111"),
        ("Ablaufmonat", "12"),
        ("Ablaufjahr", "2030"),
        ("Prüfnummer", "123"),
    ] {
        assert_eq!(kv(card, key), Some(value), "{key}");
    }
    let me = by_title(&model, "Me");
    assert!(me.tags.contains(&"identity".to_string()));
    assert_eq!(
        kv(me, "Sozialversicherungsnummer"),
        Some(format!("SSN-{MARKER}").as_str())
    );
    assert_eq!(kv(me, "Passnummer"), Some(format!("P-{MARKER}").as_str()));
    assert_eq!(
        kv(me, "Führerscheinnummer"),
        Some(format!("L-{MARKER}").as_str())
    );
    assert_eq!(kv(me, "Anrede"), Some("Dr"));
    assert_eq!(kv(me, "Postleitzahl"), Some("12345"));
    let ssh = by_title(&model, "Deploy key");
    assert!(ssh.tags.contains(&"ssh-key".to_string()));
    assert!(kv(ssh, "SSH privater Schlüssel").is_some());
    assert_eq!(kv(ssh, "SSH Fingerabdruck"), Some("SHA256:fake"));
    let unknown = by_title(&model, "Future thing");
    assert!(unknown.tags.contains(&"Bitwarden-Typ 99".to_string()));
    assert_eq!(kv(unknown, "Bitwarden: another"), Some("value"));
    assert!(kv(unknown, "Bitwarden: newThing").is_some_and(|v| v.contains("\"two\"")));
    let note = by_title(&model, "Wifi note");
    assert!(note.tags.contains(&"secure-note".to_string()));
    assert_eq!(note.note.as_deref(), Some("SSID: home\nkey inside"));
}

#[test]
fn a_deleted_entry_is_trashed_and_remembers_its_folder() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    let deleted = by_title(&model, "Deleted login");
    assert!(deleted.trashed);
    assert!(deleted.group_ref.is_none());
    let from = deleted.trashed_from_ref.as_deref().expect("folder");
    let group = model
        .groups
        .iter()
        .find(|g| g.reference == from)
        .expect("group");
    assert_eq!(group.name, "Work");
}

#[test]
fn collections_become_tags_and_an_invalid_totp_is_kept_and_listed() {
    let model = parse(&fixture("bitwarden.json")).expect("parse");
    let shared = by_title(&model, "Shared");
    assert!(shared.tags.contains(&"Sammlung: Team".to_string()));
    let broken = by_title(&model, "Broken TOTP");
    assert_eq!(broken.otp_raw.as_deref(), Some("not a secret!"));
    assert_eq!(broken.problems.len(), 1);
    assert_eq!(broken.problems[0].kind, AttentionKind::TotpInvalid);
}

#[test]
fn an_encrypted_export_is_refused() {
    let error =
        parse(br#"{"encrypted":true,"encKeyValidation_DO_NOT_EDIT":"2.x|y|z","data":"2.a|b|c"}"#)
            .expect_err("encrypted");
    assert!(
        matches!(error, HaexError::PasswordsImportFailed { ref reason } if reason == "encrypted_export"),
        "{error:?}"
    );
}

#[test]
fn something_that_is_not_an_export_is_refused() {
    assert!(matches!(
        parse(br#"{"items": 3}"#),
        Err(HaexError::PasswordsImportFailed { .. })
    ));
    assert!(matches!(
        parse(b"{ not json"),
        Err(HaexError::PasswordsImportFailed { .. })
    ));
}

#[test]
fn the_csv_export_keeps_line_breaks_urls_and_field_lines() {
    let model = parse(&fixture("bitwarden.csv")).expect("parse");
    assert_eq!(model.items.len(), 2);
    let mail = by_title(&model, "Mail");
    assert_eq!(mail.note.as_deref(), Some("line one\nline two"));
    assert_eq!(mail.url.as_deref(), Some("https://mail.example.invalid"));
    assert_eq!(kv(mail, "URL 2"), Some("https://alt.example.invalid"));
    assert_eq!(kv(mail, "Security question"), Some("first pet"));
    assert_eq!(kv(mail, "PIN"), Some("1234"));
    assert!(mail.tags.contains(&"Favorit".to_string()));
    assert_eq!(kv(mail, "Bitwarden: Passwort erneut abfragen"), Some("ja"));
    assert_eq!(model.groups.len(), 2, "Work and Work/Email");
    let note = by_title(&model, "Wifi note");
    assert!(note.tags.contains(&"secure-note".to_string()));
    assert!(note.group_ref.is_none());
}

#[test]
fn a_csv_without_the_bitwarden_columns_is_refused() {
    assert!(matches!(
        parse(b"a,b\n1,2\n"),
        Err(HaexError::PasswordsImportFailed { .. })
    ));
}

#[test]
fn a_passkey_of_the_json_export_gets_its_public_key() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use p256::pkcs8::{EncodePrivateKey as _, EncodePublicKey as _};
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("random");
    let secret = p256::SecretKey::from_slice(&seed).expect("key");
    let private = STANDARD.encode(secret.to_pkcs8_der().expect("pkcs8").as_bytes());
    let public = STANDARD.encode(
        secret
            .public_key()
            .to_public_key_der()
            .expect("spki")
            .as_bytes(),
    );
    let json = serde_json::json!({
        "encrypted": false,
        "folders": [],
        "items": [{
            "type": 1, "name": "With passkey",
            "login": { "username": "u", "fido2Credentials": [{
                "credentialId": "11111111-2222-3333-4444-555555555555",
                "keyType": "public-key", "keyAlgorithm": "ECDSA", "keyCurve": "P-256",
                "keyValue": private, "rpId": "example.invalid", "rpName": "Example",
                "userHandle": "dXNlcg", "userName": "u", "counter": "3", "discoverable": "true",
                "creationDate": "2024-01-01T00:00:00.000Z"
            }]}
        }]
    });
    let model = parse(json.to_string().as_bytes()).expect("parse");
    let passkey = &model.items[0].passkeys[0];
    assert_eq!(passkey.public_key, public);
    assert_eq!(passkey.algorithm, -7);
    assert_eq!(passkey.sign_count, 3);
    // The credential id is the 16 bytes of the UUID in standard Base64.
    assert_eq!(
        passkey.credential_id,
        STANDARD.encode(
            uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555")
                .unwrap()
                .as_bytes()
        )
    );
    assert!(model.items[0].problems.is_empty());
}
