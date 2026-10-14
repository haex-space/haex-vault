//! The credentials of a connection in the password manager (spec 038 FR-005, research R2,
//! data-model.md "Zugangsdaten-Eintrag"): an entry owned by `storage`, so rule Z14 of spec 034
//! hides it from every caller but the user and this module. Read and written as
//! `Caller::Internal { feature: "storage" }` with a grant fixed here; new credentials are a new
//! entry (the connection points at it), and an entry no connection uses any more is deleted for
//! good.
//!
//! Whether the entry is on this device ([`state`]): `present`, `missing` when the user deleted it
//! (a delete marker or the trash) or it lacks the values of the protocol (spec 052 FR-010),
//! otherwise `syncing` (it has not arrived yet).
//!
//! A connection may also point at an entry the user chose (spec 052): it stays the user's entry,
//! with no owner; [`delete`] never removes it.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use zeroize::Zeroizing;

use super::model::{CredentialsCandidate, CredentialsEntry};
use super::{
    store, Credentials, CredentialsState, PasswordCredentials, Protocol, S3Credentials,
    SshKeyCredentials,
};
use crate::error::{HolziError, Result};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::model::{ItemInput, KeyValueInput, SecretItem};
use crate::passwords::service::PasswordsService;
use crate::passwords::usage::EntryUsage;
use crate::passwords::{ids, items, reveal};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The holzi function that owns the entries (`haex_passwords_item_details.owner`).
pub const FEATURE: &str = "storage";

/// The caller this module reads and writes the password manager as.
pub const CALLER: Caller = Caller::Internal { feature: FEATURE };

/// The tag of the entries that hold the credentials of cloud storage (spec 052 FR-006); the one
/// place of its name.
pub const TAG: &str = "haex-cloud-storage";

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
        Protocol::Local => return Err(super::store::invalid("protocol")),
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
        tags: vec![TAG.to_owned()],
        key_values,
        ..ItemInput::default()
    };
    passwords.create_owned_item(&CALLER, input).await
}

/// Adds the cloud-storage tag to a user-selected entry without changing any other tag.
pub async fn tag_selected(passwords: &PasswordsService, item_id: &str) -> Result<()> {
    passwords
        .set_tags(
            &Caller::User,
            vec![item_id.to_owned()],
            vec![TAG.to_owned()],
            Vec::new(),
        )
        .await
        .map(|_| ())
}

/// Adds the tag to storage-owned entries created before spec 052. This is idempotent; user-owned
/// entries are deliberately not changed here. The direct CRDT return type lets this run inside
/// the synchronous vault-open maintenance transaction.
pub fn backfill_owned_tags(tx: &mut CrdtTransaction<'_>) -> haex_crdt::Result<u32> {
    let tag = ids::fold(TAG);
    let existing_tag = tx
        .query_map("SELECT id, name FROM haex_passwords_tags", &[], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .into_iter()
        .find(|(_, name)| ids::fold(name) == tag)
        .map(|(id, _)| id);
    let tag_id = if let Some(id) = existing_tag {
        id
    } else {
        let id = ids::tag_id(TAG).to_string();
        tx.execute(
            "INSERT INTO haex_passwords_tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, TAG, crate::passwords::clock::now()],
        )?;
        id
    };
    let item_ids = tx.query_map(
        "SELECT id FROM haex_passwords_item_details WHERE owner = ?1",
        params![FEATURE],
        |row| row.get::<_, String>(0),
    )?;
    let mut changed = 0;
    for item_id in item_ids {
        let linked = tx
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_item_tags WHERE item_id = ?1 AND tag_id = ?2",
                params![item_id, tag_id],
                |row| row.get::<_, i64>(0),
            )?
            .unwrap_or(0);
        if linked == 0 {
            tx.execute(
                "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES (?1, ?2, ?3)",
                params![
                    ids::item_tag_id(&item_id, &tag_id).to_string(),
                    item_id,
                    tag_id
                ],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Returns the linked entry's safe display metadata, if it is still visible.
pub fn entry(q: &mut impl Query, item_id: &str) -> Result<Option<CredentialsEntry>> {
    Ok(items::headers_in_scope(q, &Scope::All, &Caller::User)?
        .into_iter()
        .find(|header| header.id == item_id)
        .map(|header| CredentialsEntry {
            id: header.id,
            title: header.title,
            owned: header.owner.as_deref() == Some(FEATURE),
        }))
}

/// The credentials an entry holds for a connection of `protocol` (spec 052 data-model.md, the one
/// rule for "fits"): S3 takes the user name as access key id and the password as secret with an
/// optional custom field `sessionToken`; WebDAV and SFTP take user name and password; SFTP without a
/// password takes the custom field `privateKey` with an optional `passphrase`. Empty values count as
/// missing; `None` when the entry does not fit.
pub fn from_item(item: &SecretItem, protocol: Protocol) -> Option<Credentials> {
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
    match (protocol, username, password) {
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
    }
}

/// The entry `item_id` with its values as this module reads it; one that is not there for it is
/// [`HolziError::StorageCredentialsUnavailable`] with its [`state`].
pub async fn read_item(
    passwords: &PasswordsService,
    db: &VaultDb,
    item_id: &str,
    protocol: Protocol,
) -> Result<SecretItem> {
    match passwords
        .read_secret_item(&CALLER, &grants(), item_id.to_owned())
        .await
    {
        Err(HolziError::PasswordsNotFound) => Err(HolziError::StorageCredentialsUnavailable {
            state: state(db, item_id, protocol).await?,
        }),
        other => other,
    }
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
    let item = read_item(passwords, db, item_id, protocol).await?;
    from_item(&item, protocol).ok_or(HolziError::StorageCredentialsUnavailable {
        state: CredentialsState::Missing,
    })
}

/// The entries the user may choose for a connection of `protocol` (spec 052 research R3): not in
/// the trash, the user's own or created by holzi for a connection, and fitting by [`from_item`].
/// Tagged entries first, then by title regardless of case. No value leaves this function.
pub async fn candidates(db: &VaultDb, protocol: Protocol) -> Result<Vec<CredentialsCandidate>> {
    if protocol == Protocol::Local {
        return Err(store::invalid("protocol"));
    }
    let mut found = db
        .read(move |q| {
            let tag = ids::fold(TAG);
            let mut found = Vec::new();
            for header in items::headers_in_scope(q, &Scope::All, &Caller::User)? {
                let owned = header.owner.as_deref() == Some(FEATURE);
                let username = header.username.clone().unwrap_or_default();
                // Values in the header first, so only likely entries are read with their secrets.
                if (header.owner.is_some() && !owned)
                    || username.is_empty()
                    || (protocol != Protocol::Sftp && !header.has_password)
                    || !reveal::secret_item(q, &header.id)?
                        .is_some_and(|item| from_item(&item, protocol).is_some())
                {
                    continue;
                }
                found.push(CredentialsCandidate {
                    tagged: header.tags.iter().any(|t| ids::fold(&t.name) == tag),
                    owned,
                    id: header.id,
                    title: header.title,
                    username,
                    url: header.url,
                });
            }
            Ok(found)
        })
        .await?;
    found.sort_by_cached_key(|c| (!c.tagged, c.title.as_deref().unwrap_or("").to_lowercase()));
    Ok(found)
}

/// Whether the entry `item_id` is here with the values `protocol` needs, deleted or without them,
/// or not yet arrived (see the module; spec 052 FR-010).
pub fn state_in(q: &mut impl Query, item_id: &str, protocol: Protocol) -> Result<CredentialsState> {
    if let Some(stored) = items::item_state(q, item_id)? {
        let fits = !stored.in_trash
            && reveal::secret_item(q, item_id)?
                .is_some_and(|item| from_item(&item, protocol).is_some());
        return Ok(if fits {
            CredentialsState::Present
        } else {
            CredentialsState::Missing
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
pub async fn state(db: &VaultDb, item_id: &str, protocol: Protocol) -> Result<CredentialsState> {
    let item_id = item_id.to_owned();
    db.read(move |q| state_in(q, &item_id, protocol).map_err(Into::into))
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
