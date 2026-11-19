//! The KeePass reader against a database built at run time (spec 034, US7, T082/T083): every row
//! of `contracts/import-mapping.md` §KeePass that the `keepass` crate can prove, the three passkey
//! algorithms, and the failures (wrong password, truncated file, not a database).

#[path = "common/kdbx_fixture.rs"]
mod kdbx_fixture;

use haex_lib::passwords::import::{
    parse, Credentials, IconRef, ImportItem, ImportModel, ImportSource,
};
use haex_lib::passwords::model::AttentionKind;
use haex_lib::HaexError;
use kdbx_fixture::{build, Fixture, KEY_FILE, MARKER, PASSWORD};
use zeroize::Zeroizing;

fn credentials() -> Credentials {
    Credentials {
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: Some(Zeroizing::new(KEY_FILE.to_vec())),
    }
}

fn read(fixture: &Fixture) -> ImportModel {
    parse(ImportSource::Keepass, &fixture.bytes, &credentials()).expect("parse the fixture")
}

fn item<'a>(model: &'a ImportModel, title: &str) -> &'a ImportItem {
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

fn failed_with(result: Result<ImportModel, HaexError>, expected: &str) {
    match result {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, expected),
        other => panic!(
            "expected {expected}, got {:?}",
            other.map(|m| m.items.len())
        ),
    }
}

#[test]
fn groups_nest_with_notes_and_icons_and_the_bin_is_marked() {
    let model = read(&build());
    let work = model
        .groups
        .iter()
        .find(|g| g.name == "Work")
        .expect("Work");
    assert_eq!(work.description.as_deref(), Some("work stuff"));
    assert_eq!(
        work.icon,
        Some(IconRef::Standard("lucide:globe".to_string()))
    );
    let servers = model
        .groups
        .iter()
        .find(|g| g.name == "Servers")
        .expect("Servers");
    assert_eq!(servers.parent_ref.as_deref(), Some(work.reference.as_str()));
    assert!(matches!(servers.icon, Some(IconRef::Custom(ref b)) if b.starts_with(b"\x89PNG")));
    let bin = model.groups.iter().find(|g| g.is_recycle_bin).expect("bin");
    let old = model.groups.iter().find(|g| g.name == "Old").expect("Old");
    assert_eq!(old.parent_ref.as_deref(), Some(bin.reference.as_str()));
}

#[test]
fn an_entry_keeps_its_fields_tags_times_and_expiry() {
    let model = read(&build());
    let mail = item(&model, "Mail");
    assert_eq!(mail.username.as_deref(), Some("alice"));
    assert_eq!(
        mail.password.as_deref(),
        Some(format!("{MARKER}-mail").as_str())
    );
    assert_eq!(mail.url.as_deref(), Some("https://mail.example.invalid"));
    assert_eq!(mail.note.as_deref(), Some("line one\nline two"));
    assert_eq!(mail.tags, vec!["work".to_string(), "mail".to_string()]);
    assert_eq!(mail.created_at.as_deref(), Some("1970-01-01T00:00:00.000Z"));
    assert_eq!(mail.expires_at.as_deref(), Some("1970-01-01"));
    assert_eq!(kv(mail, "Security question"), Some("first pet"));
    assert_eq!(kv(mail, "PIN"), Some(format!("{MARKER}-pin").as_str()));
    assert_eq!(mail.otp_raw.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert!(
        mail.problems
            .iter()
            .all(|p| p.kind == AttentionKind::AttachmentTooLarge),
        "{:?}",
        mail.problems
    );
    let work = model.groups.iter().find(|g| g.name == "Work").unwrap();
    assert_eq!(mail.group_ref.as_deref(), Some(work.reference.as_str()));
}

#[test]
fn colours_override_url_auto_type_and_custom_data_become_fields() {
    let model = read(&build());
    let mail = item(&model, "Mail");
    assert_eq!(kv(mail, "KeePass: Vordergrundfarbe"), Some("#FF0010"));
    assert_eq!(kv(mail, "KeePass: Hintergrundfarbe"), Some("#002040"));
    assert_eq!(
        kv(mail, "KeePass: URL überschreiben"),
        Some("cmd://open {URL}")
    );
    assert_eq!(kv(mail, "KeePass: Auto-Type aktiv"), Some("ja"));
    assert_eq!(
        kv(mail, "KeePass: Auto-Type Standardfolge"),
        Some("{USERNAME}{TAB}{PASSWORD}{ENTER}")
    );
    assert_eq!(
        kv(mail, "KeePass: Auto-Type Verschleierung"),
        Some("Zwischenablage")
    );
    assert_eq!(kv(mail, "KeePass: Auto-Type Fenster 1"), Some("Firefox*"));
    assert_eq!(
        kv(mail, "KeePass: Auto-Type Fenster 1 Folge"),
        Some("{PASSWORD}")
    );
    assert_eq!(
        kv(mail, "KeePass: Zusatzdaten plugin-key"),
        Some("plugin value")
    );
}

#[test]
fn attachments_come_along_and_one_above_the_limit_is_reported_by_name_and_size() {
    let model = read(&build());
    let mail = item(&model, "Mail");
    assert_eq!(mail.attachments.len(), 1);
    assert_eq!(mail.attachments[0].file_name, "readme.txt");
    assert_eq!(mail.attachments[0].bytes, b"hello attachment");
    let large: Vec<_> = mail
        .problems
        .iter()
        .filter(|p| p.kind == AttentionKind::AttachmentTooLarge)
        .collect();
    assert_eq!(large.len(), 1);
    assert_eq!(large[0].file_name.as_deref(), Some("huge.bin"));
    assert_eq!(large[0].size_mib, Some(26.0));
}

#[test]
fn custom_icons_stay_pictures_and_an_unknown_standard_icon_is_reported() {
    let model = read(&build());
    let mail = item(&model, "Mail");
    assert!(matches!(mail.icon, Some(IconRef::Custom(ref b)) if b.starts_with(b"\x89PNG")));
    let broken = item(&model, "Broken TOTP");
    assert_eq!(broken.icon, None);
    assert!(broken
        .problems
        .iter()
        .any(|p| p.kind == AttentionKind::IconNotMapped));
}

#[test]
fn totp_in_every_form_and_an_invalid_one_kept_verbatim() {
    let model = read(&build());
    let seeded = item(&model, "Seeded TOTP");
    assert_eq!(seeded.otp_raw.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(seeded.otp_period, Some(60));
    assert_eq!(seeded.otp_digits, Some(8));
    assert!(seeded.problems.is_empty());
    let broken = item(&model, "Broken TOTP");
    assert_eq!(broken.otp_raw.as_deref(), Some("not a secret!"));
    assert!(broken
        .problems
        .iter()
        .any(|p| p.kind == AttentionKind::TotpInvalid));
}

#[test]
fn a_totp_address_in_the_notes_is_found_and_the_note_stays() {
    use keepass::db::{fields, Database};
    let mut db = Database::new();
    db.root_mut().add_entry().edit(|e| {
        e.set_unprotected(fields::TITLE, "Note totp");
        e.set_unprotected(
            fields::NOTES,
            "backup code\notpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=45 end",
        );
    });
    let mut bytes = Vec::new();
    db.save(&mut bytes, kdbx_fixture::key()).expect("save");
    let model = parse(ImportSource::Keepass, &bytes, &credentials()).expect("parse");
    let entry = &model.items[0];
    assert_eq!(
        entry.otp_raw.as_deref(),
        Some("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=45")
    );
    assert!(entry.note.as_deref().unwrap().contains("backup code"));
    assert!(entry.problems.is_empty());
}

#[test]
fn passkeys_of_the_three_algorithms_get_the_public_key_the_pair_has() {
    let fixture = build();
    let model = read(&fixture);
    for (title, key, rp) in [
        ("Passkeys", &fixture.es256, "es.example.invalid"),
        ("EdDSA passkey", &fixture.eddsa, "ed.example.invalid"),
        ("RSA passkey", &fixture.rs256, "rsa.example.invalid"),
    ] {
        let entry = item(&model, title);
        assert_eq!(entry.passkeys.len(), 1, "{title}");
        let passkey = &entry.passkeys[0];
        assert_eq!(passkey.relying_party_id, rp);
        assert_eq!(passkey.algorithm, key.algorithm, "{title}");
        assert_eq!(passkey.public_key, key.public_b64, "{title}");
        assert_eq!(passkey.private_key.as_str(), key.private_b64, "{title}");
        assert!(entry.problems.is_empty(), "{title}: {:?}", entry.problems);
        // The credential id is standard Base64 of the bytes the source held.
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        assert!(STANDARD.decode(&passkey.credential_id).is_ok());
    }
    // The passkey attributes are not repeated as custom fields.
    assert!(item(&model, "Passkeys")
        .key_values
        .iter()
        .all(|k| !k.key.starts_with("KPEX_PASSKEY_")));
}

#[test]
fn an_unreadable_passkey_key_is_listed_and_the_rest_of_the_entry_arrives() {
    let model = read(&build());
    let entry = item(&model, "Unreadable passkey");
    assert!(entry
        .problems
        .iter()
        .any(|p| p.kind == AttentionKind::PasskeyKeyUnreadable));
    assert_eq!(entry.passkeys.len(), 1);
    assert_eq!(entry.passkeys[0].public_key, "");
}

#[test]
fn history_states_come_with_their_attachments_oldest_first() {
    let model = read(&build());
    let entry = item(&model, "History v3");
    assert_eq!(entry.history.len(), 3);
    let passwords: Vec<_> = entry
        .history
        .iter()
        .map(|s| s.data.password.clone().unwrap_or_default())
        .collect();
    assert!(passwords.contains(&format!("{MARKER}-v1")));
    assert!(passwords.contains(&format!("{MARKER}-v2")));
    let with_files: Vec<_> = entry
        .history
        .iter()
        .filter(|s| !s.attachments.is_empty())
        .collect();
    assert_eq!(
        with_files.len(),
        2,
        "the states after the attachment was added"
    );
    assert_eq!(with_files[0].attachments[0].file_name, "state.txt");
    assert_eq!(entry.attachments.len(), 1, "the current state has it too");
}

#[test]
fn the_recycle_bin_content_is_trashed_with_its_previous_folder() {
    let model = read(&build());
    let trashed = item(&model, "Trashed entry");
    let bin = model.groups.iter().find(|g| g.is_recycle_bin).unwrap();
    assert!(trashed.trashed);
    assert_eq!(trashed.group_ref.as_deref(), Some(bin.reference.as_str()));
    let work = model.groups.iter().find(|g| g.name == "Work").unwrap();
    assert_eq!(
        trashed.trashed_from_ref.as_deref(),
        Some(work.reference.as_str())
    );
    let old = item(&model, "Old trashed");
    assert!(old.trashed);
    assert!(!item(&model, "Mail").trashed);
    assert!(!item(&model, "Root entry").trashed);
    assert_eq!(item(&model, "Root entry").group_ref, None);
}

#[test]
fn the_settings_of_the_application_give_exactly_one_report_line() {
    let model = read(&build());
    let lines: Vec<_> = model
        .source_problems
        .iter()
        .filter(|p| p.kind == AttentionKind::SourceSetting)
        .collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn a_wrong_password_a_missing_key_file_and_no_credentials_are_wrong_credentials() {
    let fixture = build();
    let wrong = Credentials {
        password: Some(Zeroizing::new("nope".to_string())),
        key_file: Some(Zeroizing::new(KEY_FILE.to_vec())),
    };
    failed_with(
        parse(ImportSource::Keepass, &fixture.bytes, &wrong),
        "wrong_credentials",
    );
    let no_file = Credentials {
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: None,
    };
    failed_with(
        parse(ImportSource::Keepass, &fixture.bytes, &no_file),
        "wrong_credentials",
    );
    failed_with(
        parse(
            ImportSource::Keepass,
            &fixture.bytes,
            &Credentials::default(),
        ),
        "wrong_credentials",
    );
}

#[test]
fn a_truncated_file_and_text_are_corrupt() {
    let fixture = build();
    let cut = &fixture.bytes[..fixture.bytes.len() / 2];
    failed_with(parse(ImportSource::Keepass, cut, &credentials()), "corrupt");
    failed_with(
        parse(ImportSource::Keepass, b"just some text", &credentials()),
        "corrupt",
    );
}

#[test]
fn no_error_or_debug_print_carries_a_planted_value() {
    let fixture = build();
    let model = read(&fixture);
    let printed = format!("{model:?}");
    assert!(
        !printed.contains(MARKER),
        "Debug of the model leaked a value"
    );
    let wrong = Credentials {
        password: Some(Zeroizing::new("hunter2-plant".to_string())),
        key_file: None,
    };
    let text = format!(
        "{:?}",
        parse(ImportSource::Keepass, &fixture.bytes, &wrong).err()
    );
    assert!(!text.contains("hunter2-plant"));
    assert!(!format!("{wrong:?}").contains("hunter2-plant"));
}
