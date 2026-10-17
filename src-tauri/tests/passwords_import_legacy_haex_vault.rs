//! The import from haex-vault 2.x (Legacy) (spec 037): every row of the mapping contract arrives (SC-001), the
//! vault file and its `-wal` are never touched (SC-002), every failure explains itself and writes
//! nothing, a cancel leaves nothing, and a second run adds no folder, tag, passkey or preset
//! (SC-003).

// These tests read raw vault state (counts, columns) that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use chosen_files::{chosen, Paths};

#[path = "common/kdbx_fixture.rs"]
mod kdbx_fixture;
#[path = "common/legacy_haex_vault_fixture.rs"]
mod legacy_haex_vault_fixture;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use zeroize::Zeroizing;

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::import::apply::{OnDuplicate, Phase, Progress};
use holzi_lib::passwords::import::legacy_haex_vault;
use holzi_lib::passwords::import::report::render_text;
use holzi_lib::passwords::import::{Credentials, IconRef, ImportModel, ImportSource};
use holzi_lib::passwords::model::{AttentionKind, ImportReport};
use holzi_lib::passwords::service::import::ImportRequest;
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;
use legacy_haex_vault_fixture::{
    build, build_with_wal, create_schema, open, sha256_files, sha256_hex, ATTACHMENT,
    HISTORY_ATTACHMENT, ICON, MARKER, PASSWORD, WAL_TITLE,
};

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-import-legacy-haex-vault"),
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

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn text(db: &Database, sql: &str) -> Option<String> {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("text")
}

fn counts(db: &Database) -> BTreeMap<String, i64> {
    db.with_connection(|conn| {
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'haex_passwords_%'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut map = BTreeMap::new();
        for name in names {
            let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0))?;
            map.insert(name, n);
        }
        Ok(map)
    })
    .expect("counts")
}

fn request(path: &Path, password: &str) -> ImportRequest {
    ImportRequest {
        source: ImportSource::LegacyHaexVault,
        file: chosen(path),
        password: Some(Zeroizing::new(password.to_string())),
        key_file: None,
    }
}

async fn run(
    f: &Fixture,
    request: ImportRequest,
    on_duplicate: OnDuplicate,
) -> Result<ImportReport, HolziError> {
    f.service
        .import_run(
            &Caller::User,
            Paths,
            request,
            on_duplicate,
            &AtomicBool::new(false),
            &|_| {},
        )
        .await
}

fn read_model(path: &Path) -> ImportModel {
    let credentials = Credentials {
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: None,
    };
    legacy_haex_vault::read(path, &credentials)
        .expect("read")
        .model
}

fn reason(result: Result<impl Sized, HolziError>) -> String {
    match result {
        Err(HolziError::PasswordsImportFailed { reason }) => reason,
        Err(other) => panic!("unexpected error {other:?}"),
        Ok(_) => panic!("expected a failure"),
    }
}

fn source_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("source dir")
}

// --- US1: the model ------------------------------------------------------------------------------

#[test]
fn the_full_entry_arrives_with_every_field() {
    let dir = source_dir();
    let vault = build(dir.path());
    let model = read_model(&vault.path);
    assert_eq!(model.items.len(), 8);
    let full = &model.items[0];
    assert_eq!(full.title.as_deref(), Some("Full Entry"));
    assert_eq!(full.username.as_deref(), Some("alice"));
    assert_eq!(
        full.password.as_deref(),
        Some(format!("{MARKER}-full").as_str())
    );
    assert_eq!(full.note.as_deref(), Some("a note"));
    assert_eq!(full.url.as_deref(), Some("https://example.invalid"));
    assert_eq!(full.color.as_deref(), Some("#00ff00"));
    assert_eq!(full.otp_raw.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(
        (
            full.otp_digits,
            full.otp_period,
            full.otp_algorithm.as_deref()
        ),
        (Some(8), Some(60), Some("SHA256"))
    );
    assert_eq!(full.expires_at.as_deref(), Some("2027-01-31"));
    assert_eq!(
        full.autofill_aliases,
        Some(serde_json::json!({ "username": ["email", "login"] }))
    );
    assert_eq!(full.created_at.as_deref(), Some("2024-01-02T03:04:05.000Z"));
    assert_eq!(full.updated_at.as_deref(), Some("2024-02-03T04:05:06.789Z"));
    assert_eq!(full.icon, Some(IconRef::Custom(ICON.to_vec())));
    assert_eq!(full.group_ref.as_deref(), Some("g-deep"));
    assert!(!full.trashed);
    let keys: Vec<&str> = full.key_values.iter().map(|kv| kv.key.as_str()).collect();
    assert_eq!(
        keys,
        ["first", "second", "third"],
        "the order of the editor (rowid)"
    );
    assert_eq!(full.tags, ["Work", "Private"]);
    assert_eq!(full.attachments.len(), 1);
    assert_eq!(full.attachments[0].file_name, "file.txt");
    assert_eq!(full.attachments[0].bytes, ATTACHMENT);
    let kinds: Vec<AttentionKind> = full.problems.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [AttentionKind::HistoryUnreadable],
        "only the broken state"
    );
}

#[test]
fn the_history_of_all_three_shapes_arrives_in_order_and_a_broken_state_is_reported() {
    let dir = source_dir();
    let vault = build(dir.path());
    let full = read_model(&vault.path).items.remove(0);
    let titles: Vec<&str> = full
        .history
        .iter()
        .map(|s| s.data.title.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(titles, ["Full v1", "Full v2", "Full v3"]);
    assert_eq!(
        full.history[0].modified_at.as_deref(),
        Some("2023-01-01T00:00:00.000Z")
    );
    assert_eq!(full.history[0].data.icon.as_deref(), Some("lucide:key"));
    assert_eq!(full.history[0].data.tag_names, ["Work"]);
    assert_eq!(
        full.history[1].attachments.len(),
        1,
        "from snapshot_binaries"
    );
    assert_eq!(full.history[1].attachments[0].bytes, HISTORY_ATTACHMENT);
    assert_eq!(
        full.problems
            .iter()
            .filter(|p| p.kind == AttentionKind::HistoryUnreadable)
            .count(),
        1
    );
}

#[test]
fn the_passkeys_arrive_one_to_one_with_their_public_key() {
    let dir = source_dir();
    let vault = build(dir.path());
    let model = read_model(&vault.path);
    let on_entry = &model.items[0].passkeys;
    assert_eq!(on_entry.len(), 1);
    let pk = &on_entry[0];
    assert_eq!(pk.public_key, vault.passkey.public_b64);
    assert_eq!(pk.private_key.as_str(), vault.passkey.private_b64);
    assert_eq!(pk.credential_id, "Y3JlZC1mdWxs");
    assert_eq!(pk.nickname.as_deref(), Some("Laptop"));
    assert_eq!((pk.sign_count, pk.is_discoverable), (5, true));
    assert_eq!(pk.created_at.as_deref(), Some("2024-05-01T10:00:00.000Z"));
    assert_eq!(pk.last_used_at.as_deref(), Some("2024-06-01T10:00:00.000Z"));
    assert_eq!(model.passkeys.len(), 1, "the passkey of no entry");
    assert_eq!(model.passkeys[0].item_id, None);
    assert!(!model.passkeys[0].is_discoverable);
}

#[test]
fn folders_come_parents_first_and_broken_parents_go_to_the_top() {
    let dir = source_dir();
    let vault = build(dir.path());
    let model = read_model(&vault.path);
    let position = |r: &str| {
        model
            .groups
            .iter()
            .position(|g| g.reference == r)
            .unwrap_or_else(|| panic!("{r}"))
    };
    assert!(position("g-work") < position("g-mail"));
    assert!(position("g-mail") < position("g-deep"));
    assert!(position("trash") < position("g-old"));
    let work = &model.groups[position("g-work")];
    assert_eq!(
        (
            work.color.as_deref(),
            work.sort_order,
            work.description.as_deref()
        ),
        (Some("#ff0000"), Some(2), Some("Work stuff"))
    );
    assert_eq!(
        work.icon,
        Some(IconRef::Standard("lucide:briefcase".into()))
    );
    assert!(model.groups[position("trash")].is_recycle_bin);
    assert_eq!(model.groups[position("g-orphan")].parent_ref, None);
    assert_eq!(model.groups[position("g-cycle-a")].parent_ref, None);
    assert_eq!(
        model.groups[position("g-cycle-b")].parent_ref.as_deref(),
        Some("g-cycle-a")
    );
    assert_eq!(
        model
            .source_problems
            .iter()
            .filter(|p| p.kind == AttentionKind::GroupReparented)
            .count(),
        2
    );
}

#[test]
fn trash_icons_tags_and_presets_follow_the_contract() {
    let dir = source_dir();
    let vault = build(dir.path());
    let model = read_model(&vault.path);
    let by_title = |t: &str| {
        model
            .items
            .iter()
            .find(|i| i.title.as_deref() == Some(t))
            .unwrap_or_else(|| panic!("{t}"))
    };
    assert!(by_title("Trashed").trashed);
    assert!(
        by_title("In an old folder").trashed,
        "below a folder in the trash"
    );
    let null_otp = by_title("Null OTP");
    assert_eq!(
        null_otp.icon,
        Some(IconRef::Standard("lucide:landmark".into()))
    );
    assert_eq!(null_otp.otp_digits, None);
    assert!(null_otp
        .problems
        .iter()
        .any(|p| p.kind == AttentionKind::TotpInvalid));
    assert_eq!(
        by_title("Lucide icon").icon,
        Some(IconRef::Standard("lucide:key".into()))
    );
    for title in ["Unknown icon", "Missing picture"] {
        let item = by_title(title);
        assert_eq!(item.icon, None, "{title}");
        assert!(item
            .problems
            .iter()
            .any(|p| p.kind == AttentionKind::IconNotMapped));
    }
    let broken = by_title("Broken attachment");
    assert!(broken.attachments.is_empty());
    assert!(broken
        .problems
        .iter()
        .any(|p| p.kind == AttentionKind::AttachmentUnreadable
            && p.file_name.as_deref() == Some("broken.bin")));
    assert_eq!(
        model.tag_colors,
        [("Work".to_string(), "#123456".to_string())]
    );
    assert!(model
        .source_problems
        .iter()
        .any(|p| p.kind == AttentionKind::TagMerged));
    let presets: Vec<(&str, bool)> = model
        .presets
        .iter()
        .map(|p| (p.name.as_str(), p.is_default))
        .collect();
    assert_eq!(presets, [("Strong", true), ("PIN", false)]);
    assert_eq!(model.presets[1].length, 6);
}

// --- US1: through the service --------------------------------------------------------------------

#[tokio::test]
async fn the_import_writes_everything_the_contract_names() {
    let f = fixture();
    let dir = source_dir();
    let vault = build(dir.path());
    let preview = f
        .service
        .import_preview(&Caller::User, Paths, request(&vault.path, PASSWORD))
        .await
        .expect("preview");
    assert_eq!(
        (
            preview.entries,
            preview.tags,
            preview.presets,
            preview.passkeys
        ),
        (8, 2, 2, 2)
    );
    assert_eq!(preview.trashed_entries, 2);
    assert_eq!(preview.warnings[0], "legacy_haex_vault_close_first");
    assert_eq!(
        count(&f.db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );

    let report = run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("import");
    assert_eq!((report.imported, report.trashed), (8, 2));
    assert_eq!(
        text(
            &f.db,
            "SELECT color FROM haex_passwords_item_details WHERE title = 'Full Entry'"
        ),
        Some("#00ff00".into())
    );
    let aliases = text(
        &f.db,
        "SELECT autofill_aliases FROM haex_passwords_item_details WHERE title = 'Full Entry'",
    )
    .expect("aliases");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&aliases).expect("json"),
        serde_json::json!({ "username": ["email", "login"] })
    );
    assert_eq!(
        text(
            &f.db,
            "SELECT icon FROM haex_passwords_item_details WHERE title = 'Full Entry'"
        ),
        Some(format!("binary:{}", sha256_hex(ICON)))
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups \
             WHERE name = 'Work' AND color = '#ff0000' AND sort_order = 2"
        ),
        1
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE name = 'Work' AND color = '#123456'"
        ),
        1
    );
    let bytes: Vec<u8> =
        f.db.with_connection(|c| {
            Ok(c.query_row(
                "SELECT b.data FROM haex_passwords_binaries b \
                 JOIN haex_passwords_item_binaries i ON i.binary_hash = b.hash \
                 WHERE i.file_name = 'file.txt'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("attachment");
    assert_eq!(bytes, ATTACHMENT);
    assert_eq!(
        count(
            &f.db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_passkeys \
                 WHERE item_id IS NULL AND public_key = '{}'",
                vault.passkey.public_b64
            )
        ),
        1
    );
    assert_eq!(
        count(&f.db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        2
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_generator_presets WHERE name = 'Strong' AND is_default = 1"
        ),
        1
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_group_items gi \
             JOIN haex_passwords_item_details d ON d.id = gi.item_id \
             WHERE d.title = 'Trashed' AND gi.group_id = 'trash'"
        ),
        1
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = 'Old' AND parent_id = 'trash'"
        ),
        1
    );
    let kinds: Vec<AttentionKind> = report.needs_attention.iter().map(|r| r.kind).collect();
    for kind in [
        AttentionKind::HistoryUnreadable,
        AttentionKind::GroupReparented,
        AttentionKind::TagMerged,
        AttentionKind::IconNotMapped,
        AttentionKind::AttachmentUnreadable,
        AttentionKind::TotpInvalid,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} is in the report");
    }
    assert!(!render_text(&report).contains(MARKER));
}

// --- US2: failures, the untouched source, the WAL, the cancel ------------------------------------

#[tokio::test]
async fn every_unreadable_file_explains_itself_and_writes_nothing() {
    let f = fixture();
    let before = counts(&f.db);
    let dir = source_dir();
    let vault = build(dir.path());
    assert_eq!(
        reason(run(&f, request(&vault.path, "wrong"), OnDuplicate::Skip).await),
        "legacy_haex_vault_locked"
    );

    let other = source_dir();
    let random: Vec<u8> = (0..8192u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8)
        .collect();
    let random_path = other.path().join("random.db");
    std::fs::write(&random_path, random).expect("write");
    assert_eq!(
        reason(run(&f, request(&random_path, PASSWORD), OnDuplicate::Skip).await),
        "legacy_haex_vault_locked"
    );
    let empty_path = other.path().join("empty.db");
    std::fs::write(&empty_path, b"").expect("write");
    assert_eq!(
        reason(run(&f, request(&empty_path, PASSWORD), OnDuplicate::Skip).await),
        "unsupported_format"
    );
    let plain_path = other.path().join("plain.db");
    let plain = haex_crdt::rusqlite::Connection::open(&plain_path).expect("plain");
    plain
        .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (zeroblob(8192));")
        .expect("fill");
    drop(plain);
    assert_eq!(
        reason(run(&f, request(&plain_path, PASSWORD), OnDuplicate::Skip).await),
        "unsupported_format"
    );

    let no_passwords = source_dir();
    let path = no_passwords.path().join("vault.db");
    let conn = open(&path);
    conn.execute_batch(
        "CREATE TABLE haex_settings (id TEXT PRIMARY KEY, v BLOB); \
                        INSERT INTO haex_settings VALUES ('a', zeroblob(8192));",
    )
    .expect("settings");
    drop(conn);
    assert_eq!(
        reason(run(&f, request(&path, PASSWORD), OnDuplicate::Skip).await),
        "no_passwords"
    );

    let missing_column = source_dir();
    let path = missing_column.path().join("vault.db");
    let conn = open(&path);
    create_schema(&conn);
    conn.execute_batch("ALTER TABLE haex_passwords_item_details DROP COLUMN password")
        .expect("drop column");
    drop(conn);
    assert_eq!(
        reason(run(&f, request(&path, PASSWORD), OnDuplicate::Skip).await),
        "unsupported_format"
    );
    assert_eq!(counts(&f.db), before);
}

#[tokio::test]
async fn an_unknown_column_is_reported_and_the_import_goes_on() {
    let f = fixture();
    let dir = source_dir();
    let vault = build(dir.path());
    let conn = open(&vault.path);
    conn.execute_batch("ALTER TABLE haex_passwords_item_details ADD COLUMN foo TEXT")
        .expect("column");
    drop(conn);
    let report = run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("import");
    assert!(report
        .needs_attention
        .iter()
        .any(|r| r.kind == AttentionKind::UnknownSourceData
            && r.field.as_deref() == Some("haex_passwords_item_details.foo")));
}

#[tokio::test]
async fn the_source_and_its_wal_stay_untouched_and_the_wal_arrives() {
    let f = fixture();
    let dir = source_dir();
    let (vault, writer) = build_with_wal(dir.path());
    let before = sha256_files(&vault.path);
    assert!(
        before.contains_key("vault.db-wal"),
        "the fixture keeps a -wal"
    );

    f.service
        .import_preview(&Caller::User, Paths, request(&vault.path, PASSWORD))
        .await
        .expect("preview");
    reason(run(&f, request(&vault.path, "wrong"), OnDuplicate::Skip).await);
    let cancel = AtomicBool::new(true);
    let _ = f
        .service
        .import_run(
            &Caller::User,
            Paths,
            request(&vault.path, PASSWORD),
            OnDuplicate::Skip,
            &cancel,
            &|_| {},
        )
        .await;
    let report = run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("import");
    assert_eq!(
        report.imported, 9,
        "the eight entries and the one only in the -wal"
    );
    assert_eq!(
        count(
            &f.db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_item_details WHERE title = '{WAL_TITLE}'"
            )
        ),
        1
    );
    assert_eq!(
        sha256_files(&vault.path),
        before,
        "no byte and no file changed"
    );
    drop(writer);
}

#[cfg(unix)]
#[tokio::test]
async fn a_read_only_place_is_enough() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let dir = source_dir();
    let vault = build(dir.path());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let result = run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip).await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert_eq!(result.expect("import").imported, 8);
}

#[tokio::test]
async fn a_cancel_after_the_extras_leaves_nothing() {
    let f = fixture();
    let before = counts(&f.db);
    let dir = source_dir();
    let vault = build(dir.path());
    let cancel = AtomicBool::new(false);
    let progress = |p: Progress| {
        if p.phase == Phase::Attachments {
            cancel.store(true, Ordering::SeqCst);
        }
    };
    let result = f
        .service
        .import_run(
            &Caller::User,
            Paths,
            request(&vault.path, PASSWORD),
            OnDuplicate::Create,
            &cancel,
            &progress,
        )
        .await;
    assert_eq!(reason(result), "cancelled");
    assert_eq!(
        counts(&f.db),
        before,
        "no entry, folder, tag, passkey or preset is left"
    );
}

// --- US3: a second run ---------------------------------------------------------------------------

#[tokio::test]
async fn a_second_run_adds_no_folder_tag_passkey_or_preset() {
    let f = fixture();
    let dir = source_dir();
    let vault = build(dir.path());
    run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("first");
    let before = counts(&f.db);
    let preview = f
        .service
        .import_preview(&Caller::User, Paths, request(&vault.path, PASSWORD))
        .await
        .expect("preview");
    assert_eq!(
        preview.duplicates, 6,
        "every entry outside the trash is there"
    );
    let again = run(&f, request(&vault.path, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("again");
    assert_eq!(again.skipped_duplicates, 6);
    assert_eq!(
        again.imported, 2,
        "entries in the trash are no duplicates (spec 034), so the two trashed ones come again"
    );
    let after = counts(&f.db);
    for table in [
        "haex_passwords_groups",
        "haex_passwords_tags",
        "haex_passwords_passkeys",
        "haex_passwords_generator_presets",
        "haex_passwords_binaries",
    ] {
        assert_eq!(after[table], before[table], "{table}");
    }
    assert!(again
        .needs_attention
        .iter()
        .any(|r| r.kind == AttentionKind::PasskeyDuplicate));
}
