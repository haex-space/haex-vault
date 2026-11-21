//! The haex-vault 2.x (Legacy) reader also reads a vault file written by this app before the
//! rename (spec 054, `fixtures/pre_rename_vault/README.md`). Its attachments are stored as blobs;
//! haex-vault 2.x stores them as Base64 text, which is all the reader used to accept, so the whole
//! import failed with "the file is damaged" as soon as the vault held one attachment.

use std::path::PathBuf;

use zeroize::Zeroizing;

use haex_lib::passwords::import::legacy_haex_vault;
use haex_lib::passwords::import::Credentials;

const PASSPHRASE: &str = "pre-rename-test-passphrase";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pre_rename_vault/pre_rename_vault.db")
}

#[test]
fn a_vault_from_before_the_rename_is_read_with_its_attachment() {
    let credentials = Credentials {
        password: Some(Zeroizing::new(PASSPHRASE.to_string())),
        key_file: None,
    };

    let read = legacy_haex_vault::read(&fixture(), &credentials).expect("the old vault is read");

    let mut titles: Vec<&str> = read
        .model
        .items
        .iter()
        .filter_map(|item| item.title.as_deref())
        .collect();
    titles.sort();
    assert_eq!(titles, ["Mail", "Old shop", "Router"]);
    let router = read
        .model
        .items
        .iter()
        .find(|item| item.title.as_deref() == Some("Router"))
        .expect("the Router entry");
    assert_eq!(router.password.as_deref(), Some("pre-rename-router-secret"));
    assert_eq!(router.attachments.len(), 1);
    assert_eq!(router.attachments[0].file_name, "router-notes.txt");
    assert!(!router.attachments[0].bytes.is_empty());
}

#[test]
fn the_sync_columns_of_the_old_vault_are_no_warning_but_real_losses_still_are() {
    let credentials = Credentials {
        password: Some(Zeroizing::new(PASSPHRASE.to_string())),
        key_file: None,
    };

    let read = legacy_haex_vault::read(&fixture(), &credentials).expect("the old vault is read");

    let fields: Vec<&str> = read
        .model
        .source_problems
        .iter()
        .filter_map(|problem| problem.field.as_deref())
        .collect();
    let noise: Vec<&&str> = fields
        .iter()
        .filter(|field| field.ends_with("_no_sync") || field.ends_with(".orphaned_at"))
        .collect();
    assert!(
        noise.is_empty(),
        "no warning for bookkeeping columns: {noise:?}"
    );
    // A column the import has no place for is still reported.
    assert!(
        fields.contains(&"haex_passwords_item_details.owner"),
        "the owner column is lost and says so: {fields:?}"
    );
}
