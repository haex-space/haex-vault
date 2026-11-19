//! Tests for writing an import (spec 034, US7, R12): the entries arrive with the times of the
//! source, a fatal error or a cancel leaves no row behind in any table, a problem at one place does
//! not stop the run, duplicates follow the user's choice, the trash and the history arrive.

#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use zeroize::Zeroizing;

use crate::error::HaexError;
use crate::passwords::import::apply::{run, Control, OnDuplicate, Phase, Progress, Step};
use crate::passwords::import::{
    bitwarden, duplicate_key, preview, report, ExistingKeys, IconRef, ImportAttachment,
    ImportGroup, ImportItem, ImportModel, ImportState, Problem,
};
use crate::passwords::model::{AttentionKind, ImportReport, ItemInput};
use crate::passwords::passkeys::PasskeyInput;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::{VaultDb, VaultGate};

const MARKER: &str = "SECRET-MARKER-APPLY";

fn vault() -> (tempfile::TempDir, Database, VaultDb) {
    let (dir, db) = open_test_vault();
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    (dir, db, vault_db)
}

fn bitwarden() -> ImportModel {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "passwords",
        "bitwarden.json",
    ]
    .iter()
    .collect();
    bitwarden::parse(&std::fs::read(path).expect("fixture")).expect("parse")
}

/// Row counts of every table of the password manager.
fn counts(db: &Database) -> BTreeMap<String, i64> {
    db.with_connection(|conn| {
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'haex_passwords_%'")?
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut map = BTreeMap::new();
        for name in names {
            let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0))?;
            map.insert(name, n);
        }
        Ok(map)
    })
    .expect("counts")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn text(db: &Database, sql: &str, arg: &str) -> Option<String> {
    db.with_connection(|c| Ok(c.query_row(sql, params![arg], |r| r.get::<_, Option<String>>(0))?))
        .expect("text")
}

struct Run {
    result: std::result::Result<ImportReport, HaexError>,
    events: Vec<Progress>,
}

async fn import(
    db: &VaultDb,
    model: ImportModel,
    existing: &ExistingKeys,
    on_duplicate: OnDuplicate,
    cancel_after_items: Option<u32>,
    inject: Option<&(dyn Fn(Step) -> Option<HaexError> + Send + Sync)>,
) -> Run {
    let cancel = AtomicBool::new(false);
    let events = Mutex::new(Vec::new());
    let progress = |p: Progress| {
        if let Some(limit) = cancel_after_items {
            if p.phase == Phase::Items && p.done >= limit {
                cancel.store(true, Ordering::SeqCst);
            }
        }
        events.lock().unwrap().push(p);
    };
    let control = Control {
        cancel: &cancel,
        progress: &progress,
        inject,
    };
    let result = run(db, model, existing, on_duplicate, &control).await;
    Run {
        result,
        events: events.into_inner().unwrap(),
    }
}

fn item(title: &str) -> ImportItem {
    ImportItem {
        title: Some(title.to_string()),
        username: Some("user".to_string()),
        password: Some(format!("{MARKER}-pw")),
        ..ImportItem::default()
    }
}

fn model_of(items: Vec<ImportItem>) -> ImportModel {
    ImportModel {
        items,
        ..ImportModel::default()
    }
}

#[tokio::test]
async fn the_bitwarden_fixture_arrives_with_times_trash_history_and_tags() {
    let (_dir, db, vault_db) = vault();
    let run = import(
        &vault_db,
        bitwarden(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await;
    let report = run.result.expect("import");
    assert_eq!(report.imported, 9);
    assert_eq!(report.trashed, 1);
    assert_eq!(report.history_states, 2);
    assert_eq!(report.skipped_duplicates, 0);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        9
    );
    // Times come from the source, not the import.
    assert_eq!(
        text(
            &db,
            "SELECT created_at FROM haex_passwords_item_details WHERE title = ?1",
            "Mail"
        )
        .as_deref(),
        Some("2024-01-05T08:30:00.000Z")
    );
    assert_eq!(
        text(
            &db,
            "SELECT updated_at FROM haex_passwords_item_details WHERE title = ?1",
            "Mail"
        )
        .as_deref(),
        Some("2024-03-02T10:00:00.000Z")
    );
    // The folders nest; the trash row exists and holds the deleted entry, which remembers its folder.
    let work = text(
        &db,
        "SELECT id FROM haex_passwords_groups WHERE name = ?1",
        "Work",
    )
    .expect("Work");
    let email = text(
        &db,
        "SELECT parent_id FROM haex_passwords_groups WHERE name = ?1",
        "Email",
    );
    assert_eq!(email.as_deref(), Some(work.as_str()));
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash'"
        ),
        1
    );
    let deleted = text(
        &db,
        "SELECT id FROM haex_passwords_item_details WHERE title = ?1",
        "Deleted login",
    )
    .unwrap();
    assert_eq!(
        text(
            &db,
            "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            &deleted
        )
        .as_deref(),
        Some("trash")
    );
    assert_eq!(
        text(
            &db,
            "SELECT trashed_from_group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            &deleted
        )
        .as_deref(),
        Some(work.as_str())
    );
    // History: the two source states with their times plus the current state, oldest first.
    let mail = text(
        &db,
        "SELECT id FROM haex_passwords_item_details WHERE title = ?1",
        "Mail",
    )
    .unwrap();
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = '{mail}'")
        ),
        3
    );
    assert_eq!(
        text(
            &db,
            "SELECT MIN(modified_at) FROM haex_passwords_item_snapshots WHERE item_id = ?1",
            &mail
        )
        .as_deref(),
        Some("2023-06-01T09:00:00.000Z")
    );
    // Tags: favourite, collection and secure note arrived; the same tag is one row.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE name = 'Favorit'"
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE name = 'Sammlung: Team'"
        ),
        1
    );
    // Progress reached the end of every phase it went through.
    assert!(run.events.iter().any(|p| p.phase == Phase::Groups));
    let last = run.events.last().expect("events");
    assert_eq!(last.done, last.total);
}

#[tokio::test]
async fn an_invalid_totp_is_stored_as_found() {
    let (_dir, db, vault_db) = vault();
    let run = import(
        &vault_db,
        bitwarden(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await;
    let report = run.result.expect("import");
    assert!(report
        .needs_attention
        .iter()
        .any(|r| r.kind == AttentionKind::TotpInvalid && r.title == "Broken TOTP"));
    let id = text(
        &db,
        "SELECT id FROM haex_passwords_item_details WHERE title = ?1",
        "Broken TOTP",
    )
    .unwrap();
    assert_eq!(
        text(
            &db,
            "SELECT otp_secret FROM haex_passwords_item_details WHERE id = ?1",
            &id
        )
        .as_deref(),
        Some("not a secret!")
    );
    let state = vault_db
        .read(move |q| crate::passwords::items::get_item(q, &id).map_err(Into::into))
        .await
        .expect("read")
        .expect("item");
    assert_eq!(state.otp_state, crate::passwords::model::OtpState::Invalid);
}

fn failing_at(step: Step) -> impl Fn(Step) -> Option<HaexError> + Send + Sync {
    move |s| {
        (s == step).then(|| HaexError::CrdtInit {
            reason: "injected".to_string(),
        })
    }
}

#[tokio::test]
async fn a_fatal_error_at_the_fifth_entry_leaves_no_row_in_any_table() {
    let (_dir, db, vault_db) = vault();
    let before = counts(&db);
    let inject = failing_at(Step::Item(4));
    let run = import(
        &vault_db,
        bitwarden(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        Some(&inject),
    )
    .await;
    assert!(run.result.is_err());
    assert_eq!(counts(&db), before, "every table is as it was");
    assert!(run.events.iter().any(|p| p.phase == Phase::Rollback));
}

fn with_attachments() -> ImportModel {
    let mut first = item("First");
    first.attachments = vec![ImportAttachment {
        file_name: "a.txt".into(),
        bytes: b"first file".to_vec(),
    }];
    first.icon = Some(IconRef::Custom(b"icon bytes".to_vec()));
    first.tags = vec!["fresh".into()];
    let mut second = item("Second");
    second.attachments = vec![ImportAttachment {
        file_name: "b.txt".into(),
        bytes: b"second file".to_vec(),
    }];
    second.history = vec![ImportState {
        modified_at: Some("2020-01-01T00:00:00.000Z".into()),
        data: crate::passwords::snapshots::SnapshotData {
            title: Some("Second".into()),
            ..Default::default()
        },
        attachments: vec![ImportAttachment {
            file_name: "old.txt".into(),
            bytes: b"old file".to_vec(),
        }],
    }];
    model_of(vec![first, second])
}

#[tokio::test]
async fn a_fatal_error_at_an_attachment_leaves_no_row_in_any_table_binaries_and_tags_included() {
    let (_dir, db, vault_db) = vault();
    let before = counts(&db);
    let inject = failing_at(Step::Attachment(2));
    let run = import(
        &vault_db,
        with_attachments(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        Some(&inject),
    )
    .await;
    assert!(run.result.is_err());
    assert_eq!(counts(&db), before);
}

#[tokio::test]
async fn a_cancel_does_the_same() {
    let (_dir, db, vault_db) = vault();
    let before = counts(&db);
    let run = import(
        &vault_db,
        bitwarden(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        Some(3),
        None,
    )
    .await;
    match run.result {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, "cancelled"),
        other => panic!("expected a cancel, got {other:?}"),
    }
    assert_eq!(counts(&db), before);
}

#[tokio::test]
async fn attachments_icons_and_history_files_arrive_each_in_its_own_write() {
    let (_dir, db, vault_db) = vault();
    let run = import(
        &vault_db,
        with_attachments(),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await;
    run.result.expect("import");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_binaries"),
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_snapshot_binaries WHERE file_name = 'old.txt'"
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE type = 'attachment'"
        ),
        3
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE type = 'icon'"
        ),
        1
    );
    let first = text(
        &db,
        "SELECT id FROM haex_passwords_item_details WHERE title = ?1",
        "First",
    )
    .unwrap();
    let icon = text(
        &db,
        "SELECT icon FROM haex_passwords_item_details WHERE id = ?1",
        &first,
    )
    .unwrap();
    assert!(icon.starts_with("binary:"), "{icon}");
    // The current state is the newest snapshot and names the attachment.
    let newest = text(
        &db,
        "SELECT snapshot_data FROM haex_passwords_item_snapshots WHERE item_id = ?1 ORDER BY rowid DESC LIMIT 1",
        &first,
    )
    .unwrap();
    assert!(newest.contains("a.txt"), "{newest}");
    // One snapshot for an entry with one attachment, not one per step.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = '{first}'"
            )
        ),
        1
    );
}

#[tokio::test]
async fn equal_bytes_are_one_binary() {
    let (_dir, db, vault_db) = vault();
    let mut a = item("A");
    let mut b = item("B");
    for i in [&mut a, &mut b] {
        i.attachments = vec![ImportAttachment {
            file_name: "same.txt".into(),
            bytes: b"same".to_vec(),
        }];
    }
    import(
        &vault_db,
        model_of(vec![a, b]),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await
    .result
    .expect("import");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        1
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_binaries"),
        2
    );
}

#[tokio::test]
async fn a_problem_at_one_place_does_not_stop_the_import_and_is_reported_without_a_value() {
    let (_dir, db, vault_db) = vault();
    let mut broken = item("Has a problem");
    broken.group_ref = Some("g1".into());
    broken.problems = vec![
        Problem::too_large("huge.bin", 26 * 1024 * 1024),
        Problem::field(
            AttentionKind::PasskeyKeyUnreadable,
            "passkey example.invalid",
        ),
    ];
    let mut model = model_of(vec![broken, item("Fine")]);
    model.groups.push(ImportGroup {
        reference: "g1".into(),
        parent_ref: None,
        name: "Top".into(),
        description: None,
        icon: None,
        is_recycle_bin: false,
        previous_parent_ref: None,
        color: None,
        sort_order: None,
    });
    model
        .source_problems
        .push(Problem::new(AttentionKind::SourceSetting));
    let report = import(
        &vault_db,
        model,
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await
    .result
    .expect("import");
    assert_eq!(report.imported, 2);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        2
    );
    let row = report
        .needs_attention
        .iter()
        .find(|r| r.kind == AttentionKind::AttachmentTooLarge)
        .expect("row");
    assert_eq!(row.title, "Has a problem");
    assert_eq!(row.folder_path, "Top");
    assert_eq!(row.file_name.as_deref(), Some("huge.bin"));
    assert_eq!(row.size_mib, Some(26.0));
    assert!(report
        .needs_attention
        .iter()
        .any(|r| r.kind == AttentionKind::SourceSetting));
    assert!(!format!("{report:?}").contains(MARKER));
    assert!(!report::render_text(&report).contains(MARKER));
}

fn existing_with(title: &str) -> ExistingKeys {
    let mut keys = ExistingKeys::new();
    keys.insert(duplicate_key(Some(title), Some("user"), None));
    keys
}

#[tokio::test]
async fn duplicates_are_skipped_or_created_as_asked() {
    let (_dir, db, vault_db) = vault();
    let model = || model_of(vec![item("Same"), item("New")]);
    let skipped = import(
        &vault_db,
        model(),
        &existing_with("Same"),
        OnDuplicate::Skip,
        None,
        None,
    )
    .await
    .result
    .expect("skip");
    assert_eq!((skipped.imported, skipped.skipped_duplicates), (1, 1));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        1
    );
    let created = import(
        &vault_db,
        model(),
        &existing_with("Same"),
        OnDuplicate::Create,
        None,
        None,
    )
    .await
    .result
    .expect("create");
    assert_eq!((created.imported, created.skipped_duplicates), (2, 0));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        3
    );
}

fn passkey(credential: &str) -> PasskeyInput {
    PasskeyInput {
        item_id: None,
        credential_id: credential.to_string(),
        relying_party_id: "example.invalid".into(),
        relying_party_name: None,
        user_name: None,
        user_display_name: None,
        user_handle: String::new(),
        private_key: Zeroizing::new(format!("{MARKER}-key")),
        public_key: String::new(),
        algorithm: -7,
        sign_count: 0,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    }
}

#[tokio::test]
async fn a_passkey_with_a_known_credential_id_is_not_created_twice_and_is_listed() {
    let (_dir, db, vault_db) = vault();
    let mut a = item("A");
    a.passkeys = vec![passkey("cred-1")];
    let mut b = item("B");
    b.passkeys = vec![passkey("cred-1")];
    let report = import(
        &vault_db,
        model_of(vec![a, b]),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        None,
    )
    .await
    .result
    .expect("import");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        1
    );
    let rows: Vec<_> = report
        .needs_attention
        .iter()
        .filter(|r| r.kind == AttentionKind::PasskeyDuplicate)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "B");
}

#[tokio::test]
async fn the_import_does_not_touch_what_the_vault_already_holds() {
    let (_dir, db, vault_db) = vault();
    let existing = vault_db
        .write(|tx| {
            crate::passwords::items::create_item(
                tx,
                &ItemInput {
                    title: Some("Mine".into()),
                    tags: vec!["mine".into()],
                    ..ItemInput::default()
                },
                None,
            )
            .map_err(Into::into)
        })
        .await
        .expect("existing");
    let before = counts(&db);
    let inject = failing_at(Step::Item(1));
    let _ = import(
        &vault_db,
        model_of(vec![item("A"), item("B")]),
        &ExistingKeys::new(),
        OnDuplicate::Create,
        None,
        Some(&inject),
    )
    .await;
    assert_eq!(counts(&db), before);
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = '{existing}'")
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE name = 'mine'"
        ),
        1
    );
}

#[test]
fn the_preview_counts_what_the_file_holds_and_names_the_problems() {
    let model = bitwarden();
    let mut existing = ExistingKeys::new();
    existing.insert(duplicate_key(
        Some("Mail"),
        Some("alice@example.invalid"),
        Some("https://mail.example.invalid"),
    ));
    let preview = preview(&model, &existing);
    assert_eq!(preview.entries, 9);
    assert_eq!(preview.groups, 2);
    assert_eq!(preview.trashed_entries, 1);
    assert_eq!(preview.history_states, 2);
    assert_eq!(preview.duplicates, 1);
    assert!(preview.warnings.contains(&"totp_invalid".to_string()));
}
