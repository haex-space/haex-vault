//! Integration coverage for the password manager through its public service (spec 034): what a
//! create writes is CRDT-tracked on every table it touches and shares one transaction, a partial
//! update leaves what it does not mention, and no result, error or `Debug` print carries a planted
//! secret value (FR-040).

// These tests read raw vault state (CRDT columns, counts) that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use chosen_files::{chosen, Paths};

use std::sync::Arc;

use haex_crdt::rusqlite::{params, ToSql};
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::{
    CopyField, ItemInput, ItemPatch, KeyValueInput, Patch, SecretField,
};
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;

const MARKER: &str = "SECRET-MARKER-ROUNDTRIP";

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-roundtrip"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    Fixture {
        _dir: dir,
        db,
        service: PasswordsService::new(vault_db),
    }
}

fn input() -> ItemInput {
    ItemInput {
        title: Some("Mail".to_string()),
        username: Some("alice".to_string()),
        password: Some(format!("{MARKER}-password")),
        note: Some(format!("{MARKER}-note")),
        otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        tags: vec!["work".to_string()],
        key_values: vec![KeyValueInput {
            key: "PIN".to_string(),
            value: Some(format!("{MARKER}-pin")),
        }],
        ..ItemInput::default()
    }
}

/// The row HLC and column HLCs of the one row `table WHERE where_sql`, after asserting that the row
/// is stamped and the table marked dirty.
fn hlc_of(db: &Database, table: &str, where_sql: &str, args: &[&dyn ToSql]) -> String {
    let (hlc, columns, dirty): (Option<String>, Option<String>, i64) = db
        .with_connection(|conn| {
            let (hlc, columns) = conn.query_row(
                &format!(
                    "SELECT haex_hlc_no_sync, haex_column_hlcs_no_sync FROM {table} WHERE {where_sql}"
                ),
                args,
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let dirty = conn.query_row(
                "SELECT COUNT(*) FROM haex_crdt_dirty_tables_no_sync WHERE table_name = ?1",
                params![table],
                |r| r.get(0),
            )?;
            Ok((hlc, columns, dirty))
        })
        .expect("read CRDT metadata");
    assert!(columns.is_some(), "{table}: no column HLCs");
    assert_eq!(dirty, 1, "{table}: not marked dirty");
    hlc.unwrap_or_else(|| panic!("{table}: no row HLC"))
}

#[tokio::test]
async fn create_item_stamps_every_table_it_writes_in_one_transaction() {
    let f = fixture();
    let id = f
        .service
        .create_item(&Caller::User, &[], input(), None)
        .await
        .expect("create");
    let by_id: &[&dyn ToSql] = &[&id];
    let hlcs = [
        hlc_of(&f.db, "haex_passwords_item_details", "id = ?1", by_id),
        hlc_of(
            &f.db,
            "haex_passwords_item_key_values",
            "item_id = ?1",
            by_id,
        ),
        hlc_of(&f.db, "haex_passwords_item_tags", "item_id = ?1", by_id),
        hlc_of(&f.db, "haex_passwords_tags", "name = 'work'", &[]),
    ];
    assert!(
        hlcs.iter().all(|h| h == &hlcs[0]),
        "one `VaultDb::write` is one transaction group with one HLC: {hlcs:?}"
    );
}

#[tokio::test]
async fn a_partial_update_leaves_what_it_does_not_mention() {
    let f = fixture();
    let id = f
        .service
        .create_item(&Caller::User, &[], input(), None)
        .await
        .expect("create");
    let detail = f
        .service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("detail");
    let token = detail.header.updated_at.expect("token");
    let patch = ItemPatch {
        title: Patch::Set("Work mail".to_string()),
        ..ItemPatch::default()
    };
    f.service
        .update_item(&Caller::User, &[], id.clone(), token, patch)
        .await
        .expect("update");
    let password = f
        .service
        .reveal(&Caller::User, id.clone(), SecretField::Password)
        .await
        .expect("reveal");
    assert_eq!(password.value.as_str(), format!("{MARKER}-password"));
    let after = f.service.get_item(&Caller::User, id).await.expect("detail");
    assert_eq!(after.header.title.as_deref(), Some("Work mail"));
    assert!(after.header.has_password && after.has_otp_secret);
}

#[tokio::test]
async fn no_result_error_or_debug_print_carries_a_planted_value() {
    let f = fixture();
    let id = f
        .service
        .create_item(&Caller::User, &[], input(), None)
        .await
        .expect("create");

    let overview = f
        .service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let detail = f
        .service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("detail");
    let revealed = f
        .service
        .reveal(&Caller::User, id.clone(), SecretField::Password)
        .await
        .expect("reveal");
    let copied = f
        .service
        .copy_value(&Caller::User, id.clone(), CopyField::Password)
        .await
        .expect("copy");
    let code = f
        .service
        .totp_code(&Caller::User, &[], id.clone())
        .await
        .expect("code");

    // The overview and the detail carry no secret; the detail carries the note (it is no secret
    // field), so the check is on the secret markers.
    for text in [
        serde_json::to_string(&overview).expect("json"),
        format!("{overview:?}"),
    ] {
        assert!(!text.contains(MARKER), "{text}");
    }
    // Custom values show unmasked in the window (spec 034 FR-005 as amended), so the detail
    // carries the PIN but never the password; its debug print carries neither (the note is no
    // secret).
    let detail_json = serde_json::to_string(&detail).expect("json");
    assert!(!detail_json.contains("-password"), "{detail_json}");
    assert!(detail_json.contains("-pin"), "{detail_json}");
    let detail_debug = format!("{detail:?}");
    assert!(
        !detail_debug.contains("-pin") && !detail_debug.contains("-password"),
        "{detail_debug}"
    );
    // The values that are meant to leave do so in redacting types.
    for text in [
        format!("{revealed:?}"),
        format!("{copied:?}"),
        format!("{code:?}"),
    ] {
        assert!(!text.contains(MARKER), "{text}");
        assert!(!text.contains("JBSWY3DPEHPK3PXP"), "{text}");
    }

    // Errors never carry a value either.
    let errors: Vec<HolziError> = vec![
        f.service
            .get_item(&Caller::User, "missing".to_string())
            .await
            .expect_err("missing"),
        f.service
            .get_item(&Caller::BuiltinAgent, id.clone())
            .await
            .expect_err("forbidden"),
        f.service
            .reveal(&Caller::BuiltinAgent, id.clone(), SecretField::Password)
            .await
            .map(|_| ())
            .expect_err("forbidden"),
        f.service
            .create_item(
                &Caller::User,
                &[],
                ItemInput {
                    password: Some(format!("{MARKER}-x")),
                    otp_secret: Some("not base32 !".to_string()),
                    ..ItemInput::default()
                },
                None,
            )
            .await
            .map(|_| ())
            .expect_err("invalid otp"),
    ];
    for error in errors {
        let text = format!(
            "{error:?} {error} {}",
            serde_json::to_string(&error).expect("json")
        );
        assert!(!text.contains(MARKER), "{text}");
    }
}

#[tokio::test]
async fn a_bulk_move_and_a_bulk_tag_change_are_tracked_for_sync() {
    use holzi_lib::passwords::model::{Target, TargetKind};
    let f = fixture();
    let first = f
        .service
        .create_item(&Caller::User, &[], ItemInput::default(), None)
        .await
        .expect("first");
    let second = f
        .service
        .create_item(&Caller::User, &[], ItemInput::default(), None)
        .await
        .expect("second");
    let folder = f
        .service
        .create_group(&Caller::User, "Folder".to_string(), None, None, None, None)
        .await
        .expect("folder");
    let moved = f
        .service
        .move_targets(
            &Caller::User,
            [&first, &second]
                .map(|id| Target {
                    kind: TargetKind::Item,
                    id: id.clone(),
                })
                .to_vec(),
            Some(folder.clone()),
        )
        .await
        .expect("move");
    assert_eq!(moved, 2);
    let changed = f
        .service
        .set_tags(
            &Caller::User,
            vec![first.clone(), second.clone()],
            vec!["Bulk".to_string()],
            vec![],
        )
        .await
        .expect("tags");
    assert_eq!(changed, 2);
    for item in [&first, &second] {
        let by_item: &[&dyn ToSql] = &[item];
        hlc_of(&f.db, "haex_passwords_group_items", "item_id = ?1", by_item);
        hlc_of(&f.db, "haex_passwords_item_tags", "item_id = ?1", by_item);
    }
    hlc_of(&f.db, "haex_passwords_groups", "id = ?1", &[&folder]);
    // Tags can be renamed and deleted by the user alone.
    let overview = f
        .service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let tag = overview
        .tags
        .iter()
        .find(|t| t.name == "Bulk")
        .expect("tag")
        .id
        .clone();
    let outside = Caller::Extension {
        id: "e".to_string(),
    };
    assert!(matches!(
        f.service
            .rename_tag(&outside, tag.clone(), "x".to_string())
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service.delete_tag(&outside, tag).await,
        Err(HolziError::PasswordsForbidden)
    ));
}

#[tokio::test]
async fn deleting_for_good_leaves_one_delete_marker_per_removed_row() {
    use holzi_lib::passwords::model::{Target, TargetKind};
    let f = fixture();
    let id = f
        .service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                tags: vec!["a".to_string(), "b".to_string()],
                key_values: vec![
                    KeyValueInput {
                        key: "one".to_string(),
                        value: Some("1".to_string()),
                    },
                    KeyValueInput {
                        key: "two".to_string(),
                        value: Some("2".to_string()),
                    },
                ],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create");
    // A second state, so the history has two rows to remove.
    let detail = f
        .service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("detail");
    f.service
        .update_item(
            &Caller::User,
            &[],
            id.clone(),
            detail.header.updated_at.expect("token"),
            ItemPatch {
                title: Patch::Set("changed".to_string()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("update");
    let target = vec![Target {
        kind: TargetKind::Item,
        id: id.clone(),
    }];
    f.service
        .trash_targets(&Caller::User, target.clone())
        .await
        .expect("trash");
    f.service
        .delete_permanently(&Caller::User, target, false)
        .await
        .expect("delete");
    let markers = |table: &str| -> i64 {
        f.db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = ?1 \
                 AND haex_hlc_no_sync IS NOT NULL",
                params![table],
                |r| r.get(0),
            )?)
        })
        .expect("markers")
    };
    assert_eq!(markers("haex_passwords_item_details"), 1);
    assert_eq!(markers("haex_passwords_item_tags"), 2);
    assert_eq!(markers("haex_passwords_item_key_values"), 2);
    assert_eq!(markers("haex_passwords_item_snapshots"), 2);
    assert_eq!(markers("haex_passwords_group_items"), 1);
}

#[tokio::test]
async fn attachments_of_the_largest_size_go_through_the_service_one_transaction_each() {
    use holzi_lib::passwords::ATTACHMENT_LIMIT_BYTES;
    let f = fixture();
    let id = f
        .service
        .create_item(&Caller::User, &[], ItemInput::default(), None)
        .await
        .expect("create");
    let dir = tempfile::tempdir().expect("dir");
    let big = dir.path().join("big.bin");
    let mut bytes = vec![0u8; ATTACHMENT_LIMIT_BYTES as usize];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    std::fs::write(&big, &bytes).expect("write");
    let first = f
        .service
        .attachment_add(&Caller::User, id.clone(), Paths, chosen(&big))
        .await
        .expect("a 25 MiB attachment fits its own transaction");
    assert_eq!(first.size, ATTACHMENT_LIMIT_BYTES);

    // A second attachment of different data is a second transaction.
    let small = dir.path().join("note.txt");
    std::fs::write(&small, b"hello").expect("write");
    let second = f
        .service
        .attachment_add(&Caller::User, id.clone(), Paths, chosen(&small))
        .await
        .expect("second attachment");
    assert_ne!(first.binary_hash, second.binary_hash);

    // One byte over the limit is refused before it is read.
    let over = dir.path().join("over.bin");
    std::fs::write(&over, vec![1u8; ATTACHMENT_LIMIT_BYTES as usize + 1]).expect("write");
    assert!(matches!(
        f.service
            .attachment_add(&Caller::User, id.clone(), Paths, chosen(&over))
            .await,
        Err(HolziError::PasswordsAttachmentTooLarge { .. })
    ));

    // The data comes back byte for byte, and a non-image has no preview.
    let saved = dir.path().join("saved.bin");
    f.service
        .attachment_save(&Caller::User, first.id.clone(), Paths, chosen(&saved))
        .await
        .expect("save");
    assert_eq!(std::fs::read(&saved).expect("read"), bytes);
    assert!(f
        .service
        .attachment_preview(&Caller::User, second.id.clone())
        .await
        .is_err());

    // An agent has no entrance to attachments (rule Z11).
    assert!(matches!(
        f.service
            .attachment_remove(&Caller::BuiltinAgent, first.id.clone())
            .await,
        Err(HolziError::PasswordsForbidden)
    ));
    f.service
        .attachment_remove(&Caller::User, first.id)
        .await
        .expect("remove");
}
