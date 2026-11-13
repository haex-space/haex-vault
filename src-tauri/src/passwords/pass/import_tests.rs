//! Importing a pass from a file through the service (spec 092, T063/T066, FR-028 to FR-033): the
//! preview keeps the file in memory and writes nothing; the save writes the pass with its folder,
//! tag, logo and codes in one transaction; a pass the vault holds already is asked about; the
//! preview and the report hold no value of a code.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::Database;

use super::import_model::{PassImportChoices, PassOnDuplicate};
use super::pkpass::read;
use super::pkpass_fixtures::{file, train, PNG};
use super::previews::PassImportPreviews;
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::model::{Target, TargetKind};
use crate::passwords::service::PasswordsService;
use crate::passwords::test_support::open_test_vault;
use crate::passwords::{trash, WALLET_GROUP_ID};
use crate::vault_gate::VaultGate;

const MARKER: &str = "SECRET-MARKER-pass-import";

fn setup() -> (tempfile::TempDir, Arc<Database>, PasswordsService) {
    let (dir, db) = open_test_vault();
    let db = Arc::new(db);
    let vault = VaultGate::new().vault_db(db.clone()).expect("vault");
    (dir, db, PasswordsService::new(vault))
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn secret_train() -> Vec<u8> {
    let mut pass = train();
    pass["barcodes"][0]["message"] = serde_json::json!(MARKER);
    file(&pass, &[("logo.png", PNG)])
}

async fn preview(
    service: &PasswordsService,
    previews: &PassImportPreviews,
    bytes: &[u8],
) -> super::import_model::PassImportPreview {
    let parsed = read(bytes, "en").expect("read");
    service
        .pass_import_preview(&Caller::User, previews, parsed)
        .await
        .expect("preview")
}

#[tokio::test]
async fn a_preview_writes_nothing_and_shows_no_code_value() {
    let (_dir, db, service) = setup();
    let previews = PassImportPreviews::new();
    let shown = preview(&service, &previews, &secret_train()).await;
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
        0
    );
    assert_eq!(shown.pass.codes.len(), 2);
    assert_eq!(shown.proposal.tag_name.as_deref(), Some("Ticket"));
    assert_eq!(shown.proposal.folder_path, None);
    assert!(shown.duplicate_of.is_none());
    assert!(!format!("{shown:?}").contains(MARKER));
    assert_eq!(previews.len(), 1);
}

#[tokio::test]
async fn saving_writes_the_pass_with_its_place_logo_and_codes() {
    let (_dir, db, service) = setup();
    let previews = PassImportPreviews::new();
    let shown = preview(&service, &previews, &secret_train()).await;
    let saved = service
        .pass_import_save(
            &Caller::User,
            &previews,
            shown.preview_id,
            PassImportChoices {
                folder_path: vec!["Trips".to_owned(), "Rail".to_owned()],
                tag_name: Some("Ticket".to_owned()),
                ..PassImportChoices::default()
            },
        )
        .await
        .expect("save");
    let id = saved.id.expect("an entry");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE type = 'icon'"
        ),
        1
    );
    let (source, kind, folder): (String, String, String) = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT d.source_id, d.pass_kind, g.name FROM haex_passwords_pass_details d \
                 JOIN haex_passwords_group_items gi ON gi.item_id = d.item_id \
                 JOIN haex_passwords_groups g ON g.id = gi.group_id WHERE d.item_id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .expect("row");
    assert_eq!(source, "pass.example.rail/S-1");
    assert_eq!(kind, "boarding_pass");
    assert_eq!(folder, "Rail");
    // The folders sit below the root of Wallet, and the tag exists once.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_groups WHERE parent_id = '{WALLET_GROUP_ID}'"
            )
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE name = 'Ticket'"
        ),
        1
    );
    // The preview is gone, and the report holds no code and no secret.
    assert!(previews.is_empty());
    let report = format!("{:?}", saved.report);
    assert!(!report.contains(MARKER), "{report}");
    assert!(!report.contains("token-token"), "{report}");
    assert_eq!(
        saved.report.title,
        "Example Rail: ICE Berlin → München, 2026-11-14"
    );
}

#[tokio::test]
async fn the_same_file_twice_asks_and_the_choice_decides() {
    let (_dir, db, service) = setup();
    let previews = PassImportPreviews::new();
    let bytes = secret_train();
    let first = preview(&service, &previews, &bytes).await;
    let saved = service
        .pass_import_save(
            &Caller::User,
            &previews,
            first.preview_id,
            PassImportChoices::default(),
        )
        .await
        .expect("first");
    let first_id = saved.id.expect("entry");

    let second = preview(&service, &previews, &bytes).await;
    assert_eq!(
        second.duplicate_of.as_ref().map(|d| d.id.as_str()),
        Some(first_id.as_str())
    );
    // Ask: refused with the id of the existing entry, the preview stays for another choice.
    let asked = service
        .pass_import_save(
            &Caller::User,
            &previews,
            second.preview_id.clone(),
            PassImportChoices::default(),
        )
        .await;
    assert!(
        matches!(&asked, Err(HolziError::PassRefused { code, .. }) if code == "duplicate"),
        "{asked:?}"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        1
    );
    // Skip: nothing written.
    let skipped = service
        .pass_import_save(
            &Caller::User,
            &previews,
            second.preview_id.clone(),
            PassImportChoices {
                on_duplicate: PassOnDuplicate::Skip,
                ..PassImportChoices::default()
            },
        )
        .await
        .expect("skip");
    assert!(skipped.id.is_none());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        1
    );
    // Add anyway: a second entry.
    let third = preview(&service, &previews, &bytes).await;
    let added = service
        .pass_import_save(
            &Caller::User,
            &previews,
            third.preview_id,
            PassImportChoices {
                on_duplicate: PassOnDuplicate::Add,
                ..PassImportChoices::default()
            },
        )
        .await
        .expect("add");
    assert!(added.id.is_some());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        2
    );
}

#[tokio::test]
async fn a_copy_in_the_trash_is_not_a_duplicate() {
    let (_dir, db, service) = setup();
    let previews = PassImportPreviews::new();
    let bytes = secret_train();
    let first = preview(&service, &previews, &bytes).await;
    let id = service
        .pass_import_save(
            &Caller::User,
            &previews,
            first.preview_id,
            PassImportChoices::default(),
        )
        .await
        .expect("first")
        .id
        .expect("entry");
    db.write(|tx| {
        trash::trash(
            tx,
            &[Target {
                kind: TargetKind::Item,
                id: id.clone(),
            }],
        )
        .map_err(Into::into)
    })
    .expect("trash");
    assert!(preview(&service, &previews, &bytes)
        .await
        .duplicate_of
        .is_none());
}

#[tokio::test]
async fn the_user_may_change_title_and_start_and_a_vanished_preview_is_not_found() {
    let (_dir, _db, service) = setup();
    let previews = PassImportPreviews::new();
    let shown = preview(&service, &previews, &secret_train()).await;
    let saved = service
        .pass_import_save(
            &Caller::User,
            &previews,
            shown.preview_id.clone(),
            PassImportChoices {
                title: Some("  Berlin trip  ".to_owned()),
                start: Some(super::model::Start::Date {
                    date: "2026-11-20".to_owned(),
                }),
                ..PassImportChoices::default()
            },
        )
        .await
        .expect("save");
    assert_eq!(saved.report.title, "Berlin trip");
    let gone = service
        .pass_import_save(
            &Caller::User,
            &previews,
            shown.preview_id,
            PassImportChoices::default(),
        )
        .await;
    assert!(matches!(gone, Err(HolziError::PasswordsNotFound)));
}

#[tokio::test]
async fn only_the_user_imports() {
    let (_dir, _db, service) = setup();
    let previews = PassImportPreviews::new();
    let parsed = read(&secret_train(), "en").expect("read");
    let refused = service
        .pass_import_preview(&Caller::BuiltinAgent, &previews, parsed)
        .await;
    assert!(refused.is_err());
}
