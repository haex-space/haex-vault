//! No secret reaches the log, not even at "Ausführlich" (spec 098 SC-003, FR-009). The log is plain
//! text beside an encrypted vault, so nothing downstream protects its content. The flows are the
//! ones the spec names: opening a vault and importing the pre-rename fixture into it. Later
//! `debug!` lines of other areas must keep this test green.

// The test seeds the identity key and reads it back below the CRDT write path.
#![allow(clippy::disallowed_methods)]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use zeroize::Zeroizing;

use chosen_files::{chosen, Paths};
use haex_lib::diagnostic_log::{self, level::LogLevel};
use haex_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use haex_lib::passwords::access::Caller;
use haex_lib::passwords::import::apply::OnDuplicate;
use haex_lib::passwords::import::{legacy_haex_vault, Credentials, ImportSource};
use haex_lib::passwords::service::import::ImportRequest;
use haex_lib::passwords::service::PasswordsService;
use haex_lib::vault_gate::VaultGate;

const PASSPHRASE: &str = "pre-rename-test-passphrase";
const CONTROL: &str = "control marker of the secrets test";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pre_rename_vault/pre_rename_vault.db")
}

/// Every file of the log folder, oldest first, as one text.
fn whole_log(folder: &std::path::Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .expect("the log folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("name")
        })
        .collect();
    names.sort();
    names
        .iter()
        .map(|name| std::fs::read_to_string(folder.join(name)).expect("read a log file"))
        .collect()
}

#[tokio::test]
async fn opening_a_vault_and_importing_the_old_one_log_no_secret() {
    let log_dir = tempfile::tempdir().expect("tempdir");
    let logs = log_dir.path().join("logs");
    log::set_logger(diagnostic_log::logger()).expect("the one logger of this test binary");
    diagnostic_log::attach_folder(&logs, LogLevel::Verbose).expect("attach");
    LogLevel::Verbose.apply();
    // A control, so a green test cannot be an empty log.
    log::info!("{CONTROL}");

    // Opening a vault: the migrations and the first-time set-up run inside.
    let vault_dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: vault_dir.path().join("vault.db"),
        key: SqlCipherKey::new("diagnostic-log-secrets"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(vault_dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    // The vault's identity key (a new vault gets it when the app creates it): a known value, so
    // the test can look for it.
    let identity_secret = [0x5a_u8; 32];
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO vault_identity_secret_no_sync (id, privkey) VALUES (1, ?1)",
            haex_crdt::rusqlite::params![identity_secret.as_slice()],
        )?;
        Ok(())
    })
    .expect("store the secret key of the vault's identity");
    let identity_key: String = identity_secret
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    // Importing the pre-rename vault: reading, then writing into the open vault.
    let credentials = Credentials {
        password: Some(Zeroizing::new(PASSPHRASE.to_string())),
        key_file: None,
    };
    let read = legacy_haex_vault::read(&fixture(), &credentials).expect("the old vault is read");
    let mail = read
        .model
        .items
        .iter()
        .find(|item| item.title.as_deref() == Some("Mail"))
        .expect("the Mail entry");
    let note = mail.note.clone().expect("the Mail entry has a note");
    assert!(!note.is_empty());

    let service = PasswordsService::new(
        VaultGate::new()
            .vault_db(Arc::new(db.clone()))
            .expect("open the gate"),
    );
    service
        .import_run(
            &Caller::User,
            Paths,
            ImportRequest {
                source: ImportSource::LegacyHaexVault,
                file: chosen(&fixture()),
                password: Some(Zeroizing::new(PASSPHRASE.to_string())),
                key_file: None,
            },
            OnDuplicate::Skip,
            &AtomicBool::new(false),
            &|_| {},
        )
        .await
        .expect("the old vault is imported");
    diagnostic_log::end();

    let text = whole_log(&logs);
    assert!(text.contains(CONTROL), "the log is not empty: {text}");
    let lower = text.to_lowercase();
    for secret in [
        "pre-rename-mail-secret",
        "pre-rename-router-secret",
        "pre-rename-shop-secret",
        "JBSWY3DPEHPK3PXP",
        "4711",
        PASSPHRASE,
        note.as_str(),
        identity_key.as_str(),
    ] {
        assert!(
            !lower.contains(&secret.to_lowercase()),
            "the log holds a secret ({} bytes long, starting {:?})",
            secret.len(),
            secret.chars().take(4).collect::<String>()
        );
    }
}
