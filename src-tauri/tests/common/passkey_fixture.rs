//! The fixture of the passkey service tests (spec 036, T060): a real vault with a
//! `PasswordsService`, entries, passkeys made through the service, and the WebAuthn requests.
//! Included per binary via `#[path = "common/passkey_fixture.rs"] mod passkey_fixture;`.
#![allow(dead_code, clippy::disallowed_methods)]

use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::model::ItemInput;
use holzi_lib::passwords::model_passkeys::{
    PasskeyConfirmRequest, PasskeyCreateRequest, PasskeyCreated, PasskeyTarget, UserPresence,
};
use holzi_lib::passwords::passkey_links;
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;

pub const ORIGIN: &str = "https://login.example.com";
pub const RP: &str = "example.com";

pub struct Fixture {
    _dir: tempfile::TempDir,
    pub db: Database,
    pub service: PasswordsService,
}

pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-passkeys"),
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

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(text: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(text).expect("Base64URL")
}

pub fn extension(tag: &str, action: GrantAction) -> (Caller, Vec<Grant>) {
    (
        Caller::Extension {
            id: format!("ext-{tag}"),
        },
        vec![Grant::new(action, Scope::tags([tag]))],
    )
}

impl Fixture {
    pub async fn entry(&self, title: &str, tags: &[&str]) -> String {
        self.service
            .create_item(
                &Caller::User,
                &[],
                ItemInput {
                    title: Some(title.to_string()),
                    tags: tags.iter().map(|t| t.to_string()).collect(),
                    ..ItemInput::default()
                },
                None,
            )
            .await
            .expect("entry")
    }

    pub async fn create(&self, item_id: &str, params: &[i64]) -> PasskeyCreated {
        self.service
            .passkey_create(&Caller::User, &[], create_request(item_id, params))
            .await
            .expect("create")
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.db
            .with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .expect("count")
    }

    pub fn last_used(&self) -> Vec<Option<String>> {
        self.db
            .with_connection(|c| {
                let mut stmt =
                    c.prepare("SELECT last_used_at FROM haex_passwords_passkeys ORDER BY rowid")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
            })
            .expect("last used")
    }
}

pub fn create_request(item_id: &str, params: &[i64]) -> PasskeyCreateRequest {
    PasskeyCreateRequest {
        target: PasskeyTarget::Existing(item_id.to_string()),
        rp_id: RP.to_string(),
        rp_name: "Example".to_string(),
        origin: ORIGIN.to_string(),
        user_handle: b64(b"user-1"),
        user_name: "anna".to_string(),
        user_display_name: Some("Anna".to_string()),
        challenge: b64(b"create-challenge"),
        exclude_credentials: Vec::new(),
        pub_key_cred_params: params.to_vec(),
        discoverable: true,
        presence: UserPresence::None,
    }
}

pub fn confirm_request(allow: &[&str]) -> PasskeyConfirmRequest {
    PasskeyConfirmRequest {
        rp_id: RP.to_string(),
        origin: ORIGIN.to_string(),
        challenge: b64(b"get-challenge"),
        allow_credentials: allow.iter().map(|c| c.to_string()).collect(),
        item_id: None,
        presence: UserPresence::None,
    }
}

impl Fixture {
    /// The id of the only passkey row.
    pub fn passkey_id(&self) -> String {
        self.db
            .with_connection(|c| {
                Ok(c.query_row("SELECT id FROM haex_passwords_passkeys", [], |r| r.get(0))?)
            })
            .expect("id")
    }

    /// Shows `passkey_id` at `item_id` by a link.
    pub fn link(&self, item_id: &str, passkey_id: &str) {
        self.db
            .write(|tx| {
                passkey_links::link(tx, item_id, passkey_id)
                    .map(|_| ())
                    .map_err(haex_crdt::Error::from)
            })
            .expect("link");
    }
}
