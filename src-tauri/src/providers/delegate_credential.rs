//! The credential of a CLI delegate as an owned entry of the password manager (spec 096 FR-006,
//! research §10, `data-model.md` §3).
//!
//! The entry belongs to the haex-vault function `delegate`, so rule Z14 of spec 034 hides it from
//! every caller but the user and this module (the built-in agent never sees it). The secret is the
//! entry's password field; two custom fields say what it is (`kind`, which the adapter turns into
//! an environment variable after containment) and whom it serves (`device`: [`VAULT_SCOPE`] for a
//! credential every device of the vault may use, or the installation ID for one bound to a device).
//! Entries are found by their tag (`haex-delegate-<vendor>`) and their scope; no row of the vault
//! names an entry, so none can point at the wrong one.
//!
//! Concurrent saves can leave two entries in one scope after a sync: the newest by `updated_at`
//! wins and the next save removes the older ones, in that scope only. A save on one device never
//! touches another device's entry. An entry that cannot be found reads as [`Read::Missing`]: it
//! was deleted, or the sync has not brought it here yet, and an entry has no id the caller could
//! name to tell the two apart, so the message for that case says both.

use std::fmt;

use zeroize::Zeroizing;

use crate::cli_tools::catalog::Catalog;
use crate::error::{HaexError, Result};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::ids;
use crate::passwords::model::{ItemInput, KeyValueInput, SecretItem};
use crate::passwords::service::{Headers, PasswordsService};
use crate::vault_gate::VaultDb;

/// The haex-vault function that owns the entries (`haex_passwords_item_details.owner`) and the name
/// the delete warning of the password manager shows.
pub const FEATURE: &str = "delegate";

/// The caller this module reads and writes the password manager as.
pub const CALLER: Caller = Caller::Internal { feature: FEATURE };

/// The scope of a credential that serves every device of the vault.
pub const VAULT_SCOPE: &str = "vault";

/// The `kind` of the device code the Abacus sign-in returns.
pub const KIND_DEVICE_CODE: &str = "device_code";

/// The custom field that names what the secret is.
const KIND_FIELD: &str = "kind";

/// The custom field that names whom the secret serves.
const SCOPE_FIELD: &str = "device";

/// Whether the credential of `vendor` lives in the password manager. Claude Code and Codex still
/// keep theirs in the provider row (spec 096 plan D9); Abacus is the first that does not.
pub fn lives_in_password_manager(vendor: &str) -> bool {
    vendor == "abacusai"
}

/// The tag of the entries of `vendor`; the one place of its name.
pub fn tag_of(vendor: &str) -> String {
    format!("haex-delegate-{vendor}")
}

/// The fixed grant of this module.
fn grants() -> [Grant; 1] {
    [Grant::new(GrantAction::ReadWrite, Scope::All)]
}

/// A credential as read: what it is and its secret. `Debug` prints no value.
pub struct Credential {
    kind: String,
    secret: Zeroizing<String>,
}

impl Credential {
    /// What the secret is (`device_code`).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The secret, for the one place that hands it to a process.
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("kind", &self.kind)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// What a vendor's credential comes to when it is looked up.
#[derive(Debug)]
pub enum Read {
    Ready(Credential),
    /// No entry in this scope: it was deleted, or has not arrived through the sync yet.
    Missing,
}

/// The value of the custom field `key` of `item`.
fn field<'a>(item: &'a SecretItem, key: &str) -> Option<&'a str> {
    item.key_values
        .iter()
        .find(|pair| pair.key.as_deref() == Some(key))
        .and_then(|pair| pair.value.as_deref())
}

/// Every readable entry of `vendor`'s tag, newest first, with the scope it serves (`None` for an
/// entry without a usable `device` field, which no scope selects).
async fn entries(
    passwords: &PasswordsService,
    vendor: &str,
) -> Result<Vec<(Option<String>, SecretItem)>> {
    let tag = ids::fold(&tag_of(vendor));
    let Headers::Items(headers) = passwords.list_headers(&CALLER, &grants()).await? else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for header in headers
        .into_iter()
        .filter(|header| header.tags.iter().any(|t| ids::fold(&t.name) == tag))
    {
        match passwords
            .read_secret_item(&CALLER, &grants(), header.id)
            .await
        {
            Ok(item) => {
                let scope = field(&item, SCOPE_FIELD)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                found.push((scope, item));
            }
            // Deleted between the list and the read.
            Err(HaexError::PasswordsNotFound) => {}
            Err(error) => return Err(error),
        }
    }
    found.sort_by(|a, b| (&b.1.updated_at, &b.1.id).cmp(&(&a.1.updated_at, &a.1.id)));
    Ok(found)
}

/// Saves the credential of `vendor` for `scope` ([`VAULT_SCOPE`] or an installation ID): a new
/// entry first, then the older entries of the same scope are removed, so there is never a moment
/// without a credential. Other scopes are left alone.
pub async fn save(db: &VaultDb, vendor: &str, kind: &str, secret: &str, scope: &str) -> Result<()> {
    if kind.is_empty() || secret.is_empty() || scope.is_empty() {
        return Err(HaexError::InvalidInput {
            reason: "delegate credential".to_owned(),
        });
    }
    let passwords = PasswordsService::new(db.clone());
    let earlier: Vec<String> = entries(&passwords, vendor)
        .await?
        .into_iter()
        .filter(|(entry_scope, _)| entry_scope.as_deref() == Some(scope))
        .map(|(_, item)| item.id)
        .collect();
    let name = Catalog::shipped()
        .vendor(vendor)
        .map_or(vendor, |definition| definition.display_name.as_str())
        .to_owned();
    let input = ItemInput {
        title: Some(format!("Delegate: {name}")),
        password: Some(secret.to_owned()),
        tags: vec![tag_of(vendor)],
        key_values: vec![
            KeyValueInput {
                key: KIND_FIELD.to_owned(),
                value: Some(kind.to_owned()),
            },
            KeyValueInput {
                key: SCOPE_FIELD.to_owned(),
                value: Some(scope.to_owned()),
            },
        ],
        ..ItemInput::default()
    };
    passwords.create_owned_item(&CALLER, input).await?;
    for id in earlier {
        remove(&passwords, id).await?;
    }
    Ok(())
}

/// The credential of `vendor` for `scope`: the newest entry of that scope with a secret and a kind.
pub async fn read(db: &VaultDb, vendor: &str, scope: &str) -> Result<Read> {
    let passwords = PasswordsService::new(db.clone());
    Ok(entries(&passwords, vendor)
        .await?
        .into_iter()
        .filter(|(entry_scope, _)| entry_scope.as_deref() == Some(scope))
        .find_map(|(_, item)| {
            let secret = item.password.clone().filter(|value| !value.is_empty())?;
            let kind = field(&item, KIND_FIELD).filter(|value| !value.is_empty())?;
            Some(Credential {
                kind: kind.to_owned(),
                secret: Zeroizing::new(secret),
            })
        })
        .map_or(Read::Missing, Read::Ready))
}

/// Deletes every credential of `vendor`, in every scope: disconnecting is vault-wide. Nothing to
/// delete is fine.
pub async fn delete(db: &VaultDb, vendor: &str) -> Result<()> {
    let passwords = PasswordsService::new(db.clone());
    for (_, item) in entries(&passwords, vendor).await? {
        remove(&passwords, item.id).await?;
    }
    Ok(())
}

/// Deletes one entry for good; one that is already gone is fine.
async fn remove(passwords: &PasswordsService, id: String) -> Result<()> {
    match passwords.delete_owned_item(&CALLER, id).await {
        Ok(()) | Err(HaexError::PasswordsNotFound) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "delegate_credential_tests.rs"]
mod tests;
