//! The credentials of a connection in the password manager (spec 038 FR-005, research R2,
//! data-model.md "Zugangsdaten-Eintrag"): an entry owned by `storage`, so rule Z14 of spec 034
//! hides it from every caller but the user and this module. Read and written as
//! `Caller::Internal { feature: "storage" }` with a grant fixed here; new credentials are a new
//! entry (the connection points at it), and an entry no connection uses any more is deleted for
//! good.
//!
//! Whether the entry is on this device ([`state`]): `present`, `missing` when the user deleted it
//! (a delete marker or the trash), otherwise `syncing` (it has not arrived yet).

use haex_crdt::rusqlite::params;
use zeroize::Zeroizing;

use super::{
    store, Credentials, CredentialsState, PasswordCredentials, Protocol, S3Credentials,
    SshKeyCredentials,
};
use crate::error::{HolziError, Result};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::items;
use crate::passwords::model::{ItemInput, KeyValueInput};
use crate::passwords::service::PasswordsService;
use crate::passwords::usage::EntryUsage;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The holzi function that owns the entries (`haex_passwords_item_details.owner`).
pub const FEATURE: &str = "storage";

/// The caller this module reads and writes the password manager as.
pub const CALLER: Caller = Caller::Internal { feature: FEATURE };

/// The custom field that holds a session token.
pub const SESSION_TOKEN: &str = "sessionToken";

/// The custom field that holds a private SSH key (spec 050 data-model.md).
pub const PRIVATE_KEY: &str = "privateKey";

/// The custom field that holds the passphrase of a private SSH key.
pub const PASSPHRASE: &str = "passphrase";

/// Grants the internal storage caller read and write access for credential operations.
fn grants() -> [Grant; 1] {
    [Grant::new(GrantAction::ReadWrite, Scope::All)]
}

/// A custom field of an entry.
fn field(key: &str, value: &Zeroizing<String>) -> KeyValueInput {
    KeyValueInput {
        key: key.to_owned(),
        value: Some(value.to_string()),
    }
}

/// Creates the entry for new credentials and returns its id. The title names the protocol (spec
/// 050 data-model.md): `S3: …`, `WebDAV: …` or `SFTP: …`; a private key and its passphrase go into
/// custom fields, which the password manager treats as secrets.
pub async fn create(
    passwords: &PasswordsService,
    protocol: Protocol,
    provider_name: &str,
    endpoint: &str,
    credentials: &Credentials,
) -> Result<String> {
    let label = match protocol {
        Protocol::S3 => "S3",
        Protocol::WebDav => "WebDAV",
        Protocol::Sftp => "SFTP",
    };
    let (username, password, key_values) = match credentials {
        Credentials::S3(s3) => (
            s3.access_key_id.clone(),
            Some(s3.secret_access_key.to_string()),
            s3.session_token
                .iter()
                .map(|token| field(SESSION_TOKEN, token))
                .collect(),
        ),
        Credentials::Password(login) => (
            login.username.clone(),
            Some(login.password.to_string()),
            Vec::new(),
        ),
        Credentials::SshKey(key) => (
            key.username.clone(),
            None,
            std::iter::once(field(PRIVATE_KEY, &key.private_key))
                .chain(key.passphrase.iter().map(|text| field(PASSPHRASE, text)))
                .collect(),
        ),
    };
    let input = ItemInput {
        title: Some(format!("{label}: {}", provider_name.trim())),
        username: Some(username),
        password,
        url: (!endpoint.trim().is_empty()).then(|| endpoint.trim().to_owned()),
        key_values,
        ..ItemInput::default()
    };
    passwords.create_owned_item(&CALLER, input).await
}

/// Reads the credentials of the entry `item_id` for a connection of `protocol`. An entry that is not
/// there, or one without the fields the protocol needs, is
/// [`HolziError::StorageCredentialsUnavailable`] with its [`state`].
pub async fn read(
    passwords: &PasswordsService,
    db: &VaultDb,
    item_id: &str,
    protocol: Protocol,
) -> Result<Credentials> {
    let unavailable = |state| HolziError::StorageCredentialsUnavailable { state };
    let item = match passwords
        .read_secret_item(&CALLER, &grants(), item_id.to_owned())
        .await
    {
        Ok(item) => item,
        Err(HolziError::PasswordsNotFound) => return Err(unavailable(state(db, item_id).await?)),
        Err(error) => return Err(error),
    };
    let custom = |key: &str| {
        item.key_values
            .iter()
            .find(|kv| kv.key.as_deref() == Some(key))
            .and_then(|kv| kv.value.clone())
            .filter(|value| !value.is_empty())
            .map(Zeroizing::new)
    };
    let username = item.username.clone().filter(|name| !name.is_empty());
    let password = item.password.clone().filter(|secret| !secret.is_empty());
    let credentials = match (protocol, username, password) {
        (Protocol::S3, Some(access_key_id), Some(secret)) => Some(Credentials::S3(S3Credentials {
            access_key_id,
            secret_access_key: Zeroizing::new(secret),
            session_token: custom(SESSION_TOKEN),
        })),
        (Protocol::Sftp, Some(username), None) => custom(PRIVATE_KEY).map(|private_key| {
            Credentials::SshKey(SshKeyCredentials {
                username,
                private_key,
                passphrase: custom(PASSPHRASE),
            })
        }),
        (Protocol::WebDav | Protocol::Sftp, Some(username), Some(password)) => {
            Some(Credentials::Password(PasswordCredentials {
                username,
                password: Zeroizing::new(password),
            }))
        }
        _ => None,
    };
    credentials.ok_or_else(|| unavailable(CredentialsState::Missing))
}

/// Whether the entry `item_id` is here, deleted or not yet arrived (see the module).
pub fn state_in(q: &mut impl Query, item_id: &str) -> Result<CredentialsState> {
    if let Some(stored) = items::item_state(q, item_id)? {
        return Ok(if stored.in_trash {
            CredentialsState::Missing
        } else {
            CredentialsState::Present
        });
    }
    let deleted = q
        .query_row(
            "SELECT COUNT(*) FROM haex_deleted_rows \
             WHERE table_name = 'haex_passwords_item_details' \
               AND json_extract(row_pks, '$.id') = ?1",
            params![item_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    Ok(if deleted {
        CredentialsState::Missing
    } else {
        CredentialsState::Syncing
    })
}

/// [`state_in`] on its own read.
pub async fn state(db: &VaultDb, item_id: &str) -> Result<CredentialsState> {
    let item_id = item_id.to_owned();
    db.read(move |q| state_in(q, &item_id).map_err(Into::into))
        .await
}

/// Deletes the entry for good; one that is already gone is fine.
pub async fn delete(passwords: &PasswordsService, item_id: &str) -> Result<()> {
    match passwords
        .delete_owned_item(&CALLER, item_id.to_owned())
        .await
    {
        Ok(()) | Err(HolziError::PasswordsNotFound) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Tells the password manager that a connection uses an entry (spec 034 FR-034), so deleting
/// "S3: …" there warns first. It reads the vault that is open when it is asked.
pub struct StorageUsage<F> {
    database: F,
}

impl<F> StorageUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    /// Creates a usage check that obtains the active vault from `database` on each request.
    pub fn new(database: F) -> Self {
        Self { database }
    }
}

impl<F> EntryUsage for StorageUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    /// Identifies storage as the feature using the password entry.
    fn feature(&self) -> &'static str {
        FEATURE
    }

    /// Checks whether a connection references the entry; returns false if the vault is unavailable
    /// or the lookup fails.
    fn uses(&self, item_id: &str) -> bool {
        let Some(db) = (self.database)() else {
            return false;
        };
        db.read_blocking(|q| store::credentials_in_use(q, item_id).map_err(Into::into))
            .unwrap_or(false)
    }
}
