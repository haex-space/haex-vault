//! The import through the public service with real files (spec 034, US7, SC-010, T085): the counts
//! match the source down to the trash, nothing is lost without a line in the report, wrong
//! credentials and a corrupt file change nothing, a cancel leaves nothing, and no report or error
//! carries a planted secret.

// These tests read raw vault state (counts, columns) that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use chosen_files::{chosen, Paths};

#[path = "common/kdbx_fixture.rs"]
mod kdbx_fixture;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use zeroize::Zeroizing;

use holzi_lib::files::picked::PickedFile;
use holzi_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::import::apply::{OnDuplicate, Phase, Progress};
use holzi_lib::passwords::import::report::render_text;
use holzi_lib::passwords::import::ImportSource;
use holzi_lib::passwords::model::{AttentionKind, ImportReport};
use holzi_lib::passwords::service::import::ImportRequest;
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HaexError;
use kdbx_fixture::{build, KEY_FILE, MARKER, PASSWORD};

struct Fixture {
    dir: tempfile::TempDir,
    db: Database,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-import"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    Fixture {
        dir,
        db,
        service: PasswordsService::new(vault_db),
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/passwords")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
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

fn keepass_file(f: &Fixture) -> (String, String) {
    let kdbx = f.dir.path().join("fixture.kdbx");
    std::fs::write(&kdbx, build().bytes).expect("write kdbx");
    let key = f.dir.path().join("fixture.key");
    std::fs::write(&key, KEY_FILE).expect("write key file");
    (
        kdbx.to_string_lossy().into_owned(),
        key.to_string_lossy().into_owned(),
    )
}

fn keepass_request(f: &Fixture, password: &str) -> ImportRequest {
    let (path, key) = keepass_file(f);
    ImportRequest {
        source: ImportSource::Keepass,
        file: PickedFile(path),
        password: Some(Zeroizing::new(password.to_string())),
        key_file: Some(PickedFile(key)),
    }
}

fn file_request(source: ImportSource, name: &str) -> ImportRequest {
    ImportRequest {
        source,
        file: chosen(&fixtures_dir().join(name)),
        password: None,
        key_file: None,
    }
}

async fn run(
    f: &Fixture,
    request: ImportRequest,
    on_duplicate: OnDuplicate,
) -> Result<ImportReport, HaexError> {
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

#[tokio::test]
async fn the_keepass_counts_match_the_source_down_to_the_trash() {
    let f = fixture();
    let request = keepass_request(&f, PASSWORD);
    let preview = f
        .service
        .import_preview(&Caller::User, Paths, request)
        .await
        .expect("preview");
    assert_eq!(
        count(&f.db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0,
        "a preview writes nothing"
    );
    let report = run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Create)
        .await
        .expect("import");
    // The source holds 12 entries (root 1, work 8, servers 1, bin 1+1); all of them arrive.
    assert_eq!(preview.entries, 12);
    assert_eq!(report.imported, preview.entries);
    assert_eq!(
        count(&f.db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        12
    );
    assert_eq!(report.trashed, 2);
    assert_eq!(report.skipped_duplicates, 0);
    // Folders: Work, Servers, Old (the bin is the trash row).
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id <> 'trash'"
        ),
        3
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash'"
        ),
        1
    );
    // Both trashed entries are in the trash, one directly, one below the sub-folder.
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_group_items WHERE group_id = 'trash'"
        ),
        1
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE parent_id = 'trash'"
        ),
        1
    );
    // The history of three states and its attachment arrived beside the current state.
    let history_item: String =
        f.db.with_connection(|c| {
            Ok(c.query_row(
                "SELECT id FROM haex_passwords_item_details WHERE title = 'History v3'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("history item");
    assert_eq!(
        count(&f.db, &format!("SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = '{history_item}'")),
        4
    );
}

#[tokio::test]
async fn nothing_is_lost_without_a_line_in_the_report() {
    let f = fixture();
    let report = run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Create)
        .await
        .expect("import");
    let kinds: Vec<AttentionKind> = report.needs_attention.iter().map(|r| r.kind).collect();
    // The 26 MiB attachment, the broken TOTP, the unknown icon, the unreadable passkey key and the
    // application settings each have a row.
    for expected in [
        AttentionKind::AttachmentTooLarge,
        AttentionKind::TotpInvalid,
        AttentionKind::IconNotMapped,
        AttentionKind::PasskeyKeyUnreadable,
        AttentionKind::SourceSetting,
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
    let large = report
        .needs_attention
        .iter()
        .find(|r| r.kind == AttentionKind::AttachmentTooLarge)
        .expect("row");
    assert_eq!(large.title, "Mail");
    assert_eq!(large.folder_path, "Work");
    assert_eq!(large.file_name.as_deref(), Some("huge.bin"));
    // The attachment that fits arrived; the passkeys of the three algorithms arrived with a public key.
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_item_binaries WHERE file_name = 'readme.txt'"
        ),
        1
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_item_binaries WHERE file_name = 'huge.bin'"
        ),
        0
    );
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_passkeys WHERE public_key <> ''"
        ),
        3
    );
    // The note with a line break survived.
    let note: String =
        f.db.with_connection(|c| {
            Ok(c.query_row(
                "SELECT note FROM haex_passwords_item_details WHERE title = 'Mail'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("note");
    assert_eq!(note, "line one\nline two");
}

#[tokio::test]
async fn the_report_text_and_every_error_carry_no_planted_secret() {
    let f = fixture();
    let report = run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Create)
        .await
        .expect("import");
    assert!(!format!("{report:?}").contains(MARKER));
    let text = render_text(&report);
    assert!(!text.contains(MARKER));
    assert!(text.contains("attachment_too_large") && text.contains("huge.bin"));
    let error = run(
        &f,
        keepass_request(&f, "wrong-password-plant"),
        OnDuplicate::Create,
    )
    .await
    .expect_err("wrong");
    assert!(!format!("{error:?} {error}").contains("wrong-password-plant"));
}

#[tokio::test]
async fn wrong_credentials_and_a_corrupt_file_change_nothing() {
    let f = fixture();
    let before = counts(&f.db);
    match run(&f, keepass_request(&f, "nope"), OnDuplicate::Create).await {
        Err(HaexError::PasswordsImportFailed { reason }) => {
            assert_eq!(reason, "wrong_credentials")
        }
        other => panic!("{other:?}"),
    }
    let broken = f.dir.path().join("broken.kdbx");
    std::fs::write(&broken, b"not a database at all").expect("write");
    let request = ImportRequest {
        source: ImportSource::Keepass,
        file: chosen(&broken),
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: None,
    };
    match run(&f, request, OnDuplicate::Create).await {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, "corrupt"),
        other => panic!("{other:?}"),
    }
    let missing = ImportRequest {
        source: ImportSource::Bitwarden,
        file: chosen(&f.dir.path().join("nothing.json")),
        password: None,
        key_file: None,
    };
    match run(&f, missing, OnDuplicate::Create).await {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, "unreadable"),
        other => panic!("{other:?}"),
    }
    assert_eq!(counts(&f.db), before);
}

#[tokio::test]
async fn a_cancelled_run_leaves_nothing() {
    let f = fixture();
    let before = counts(&f.db);
    let cancel = AtomicBool::new(false);
    let progress = |p: Progress| {
        if p.phase == Phase::Items && p.done >= 3 {
            cancel.store(true, Ordering::SeqCst);
        }
    };
    let result = f
        .service
        .import_run(
            &Caller::User,
            Paths,
            keepass_request(&f, PASSWORD),
            OnDuplicate::Create,
            &cancel,
            &progress,
        )
        .await;
    match result {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, "cancelled"),
        other => panic!("{other:?}"),
    }
    assert_eq!(counts(&f.db), before);
}

#[tokio::test]
async fn bitwarden_json_csv_and_lastpass_arrive_and_a_second_run_skips_the_duplicates() {
    let f = fixture();
    let json = run(
        &f,
        file_request(ImportSource::Bitwarden, "bitwarden.json"),
        OnDuplicate::Skip,
    )
    .await
    .expect("json");
    assert_eq!(json.imported, 9);
    let again = run(
        &f,
        file_request(ImportSource::Bitwarden, "bitwarden.json"),
        OnDuplicate::Skip,
    )
    .await
    .expect("again");
    assert_eq!(
        (again.imported, again.skipped_duplicates),
        (1, 8),
        "the deleted login sits in the trash, so it is not one that is there"
    );
    let csv = run(
        &f,
        file_request(ImportSource::Bitwarden, "bitwarden.csv"),
        OnDuplicate::Create,
    )
    .await
    .expect("csv");
    assert_eq!(csv.imported, 2);
    let lastpass = run(
        &f,
        file_request(ImportSource::Lastpass, "lastpass.csv"),
        OnDuplicate::Create,
    )
    .await
    .expect("lastpass");
    assert_eq!(lastpass.imported, 3);
    assert_eq!(
        count(&f.db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        15
    );
}

#[tokio::test]
async fn a_second_keepass_run_reuses_the_folders_instead_of_doubling_them() {
    let f = fixture();
    run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("first");
    let before = counts(&f.db);
    let again = run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Skip)
        .await
        .expect("again");
    assert_eq!(
        again.imported, 2,
        "only the two entries in the trash come again"
    );
    let after = counts(&f.db);
    assert_eq!(
        after["haex_passwords_groups"], before["haex_passwords_groups"],
        "spec 037 FR-017: no folder twice"
    );
}

#[tokio::test]
async fn a_folder_the_vault_has_takes_the_entries_and_survives_a_rollback() {
    let f = fixture();
    let work = f
        .service
        .create_group(&Caller::User, "Work".into(), None, None, None, None)
        .await
        .expect("folder");
    run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Create)
        .await
        .expect("import");
    assert_eq!(
        count(
            &f.db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = 'Work'"
        ),
        1
    );
    assert!(
        count(
            &f.db,
            &format!("SELECT COUNT(*) FROM haex_passwords_group_items WHERE group_id = '{work}'")
        ) > 0,
        "the entries of Work went into the folder that was there"
    );

    let g = fixture();
    let kept = g
        .service
        .create_group(&Caller::User, "Work".into(), None, None, None, None)
        .await
        .expect("folder");
    let cancel = AtomicBool::new(false);
    let progress = |p: Progress| {
        if p.phase == Phase::Items && p.done >= 3 {
            cancel.store(true, Ordering::SeqCst);
        }
    };
    let result = g
        .service
        .import_run(
            &Caller::User,
            Paths,
            keepass_request(&g, PASSWORD),
            OnDuplicate::Create,
            &cancel,
            &progress,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        count(
            &g.db,
            &format!("SELECT COUNT(*) FROM haex_passwords_groups WHERE id = '{kept}'")
        ),
        1,
        "the rollback keeps the folder the vault had"
    );
}

#[tokio::test]
async fn an_encrypted_bitwarden_export_is_refused_and_writes_nothing() {
    let f = fixture();
    let path = f.dir.path().join("enc.json");
    std::fs::write(&path, br#"{"encrypted":true,"data":"2.a|b|c"}"#).expect("write");
    let before = counts(&f.db);
    let request = ImportRequest {
        source: ImportSource::Bitwarden,
        file: chosen(&path),
        password: None,
        key_file: None,
    };
    match run(&f, request, OnDuplicate::Create).await {
        Err(HaexError::PasswordsImportFailed { reason }) => assert_eq!(reason, "encrypted_export"),
        other => panic!("{other:?}"),
    }
    assert_eq!(counts(&f.db), before);
}

#[tokio::test]
async fn the_picture_of_an_imported_icon_comes_back_and_only_for_the_user() {
    let f = fixture();
    run(&f, keepass_request(&f, PASSWORD), OnDuplicate::Create)
        .await
        .expect("import");
    let icon: String =
        f.db.with_connection(|c| {
            Ok(c.query_row(
                "SELECT icon FROM haex_passwords_item_details WHERE title = 'Mail'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("icon");
    let hash = icon
        .strip_prefix("binary:")
        .expect("binary icon")
        .to_string();
    let bytes = f
        .service
        .icon_preview(&Caller::User, hash.clone())
        .await
        .expect("preview");
    assert!(bytes.starts_with(b"\x89PNG"));
    assert!(matches!(
        f.service.icon_preview(&Caller::BuiltinAgent, hash).await,
        Err(HaexError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service
            .icon_preview(&Caller::User, "no-such-hash".to_string())
            .await,
        Err(HaexError::PasswordsNotFound)
    ));
}

#[tokio::test]
async fn nobody_but_the_user_may_import() {
    let f = fixture();
    let request = || file_request(ImportSource::Bitwarden, "bitwarden.json");
    assert!(matches!(
        f.service
            .import_preview(&Caller::BuiltinAgent, Paths, request())
            .await,
        Err(HaexError::PasswordsForbidden)
    ));
    assert!(matches!(
        f.service
            .import_run(
                &Caller::BuiltinAgent,
                Paths,
                request(),
                OnDuplicate::Create,
                &AtomicBool::new(false),
                &|_| {}
            )
            .await,
        Err(HaexError::PasswordsForbidden)
    ));
}

#[test]
fn the_fixture_directory_holds_no_key_material() {
    let patterns = [
        "PRIVATE KEY-----",
        "BEGIN RSA",
        "BEGIN EC ",
        "BEGIN OPENSSH",
        "BEGIN ENCRYPTED",
        "ghp_",
        "xoxb-",
        "sk-ant-",
        "AKIA",
    ];
    for entry in std::fs::read_dir(fixtures_dir()).expect("dir") {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("text file");
        for pattern in patterns {
            assert!(
                !text.contains(pattern),
                "{} holds {pattern}",
                path.display()
            );
        }
        let _ = params![];
    }
}

#[tokio::test]
async fn keepass_references_become_placeholders_on_the_imported_entries() {
    use holzi_lib::passwords::model::SecretField;
    let f = fixture();
    let kdbx = f.dir.path().join("references.kdbx");
    std::fs::write(&kdbx, kdbx_fixture::with_references()).expect("write kdbx");
    let key = f.dir.path().join("fixture.key");
    std::fs::write(&key, KEY_FILE).expect("write key file");
    let request = ImportRequest {
        source: ImportSource::Keepass,
        file: chosen(&kdbx),
        password: Some(Zeroizing::new(PASSWORD.to_string())),
        key_file: Some(chosen(&key)),
    };
    let report = run(&f, request, OnDuplicate::Create).await.expect("import");
    // By id and by a unique title: converted; by a title two entries contain: text.
    assert_eq!(report.references_converted, 2);
    assert_eq!(report.references_left_as_text, 1);
    let (zweit, konto): (String, String) =
        f.db.with_connection(|c| {
            let id = |title: &str| -> Result<String, haex_crdt::rusqlite::Error> {
                c.query_row(
                    "SELECT id FROM haex_passwords_item_details WHERE title = ?1",
                    params![title],
                    |r| r.get(0),
                )
            };
            Ok((id("Zweit")?, id("Konto")?))
        })
        .expect("ids");
    let detail = f
        .service
        .get_item(&Caller::User, zweit.clone())
        .await
        .expect("zweit");
    assert_eq!(
        detail.header.username.as_deref(),
        Some(format!("{{${konto}:username}}").as_str())
    );
    assert_eq!(detail.note.as_deref(), Some("see {REF:P@T:Kont}"));
    let password = f
        .service
        .reveal(&Caller::User, zweit, SecretField::Password)
        .await
        .expect("reveal");
    assert_eq!(password.value.as_str(), format!("{MARKER}-konto"));
    assert!(!render_text(&report).contains(MARKER));
}
