//! The login of a marketplace in the password manager (spec 069 US5, research R11, FR-026 to
//! FR-029), modelled on the credentials of cloud storage (`remote_storage::credentials`): the
//! row of `haex_marketplaces` holds only the id of an entry, never a token or a password.
//!
//! An entry the form creates belongs to `marketplace`, so rule Z14 of spec 034 hides it from every
//! caller but the user and this module; one the user chose stays theirs (owner and shares
//! unchanged) and only gets the tag. [`delete`] never removes an entry of the user.
//!
//! Whether the entry is on this device ([`state_in`]): `present`, `missing` when it was deleted
//! (a delete marker or the trash) or lacks the fields the auth type needs, otherwise `syncing`
//! (it has not arrived yet).

use super::client::AuthHeader;
use super::model::{MarketplaceAuthType, MarketplaceCredentialsState, MarketplaceNewCredentials};
use crate::error::{HolziError, Result};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::model::{ItemInput, SecretItem};
use crate::passwords::service::PasswordsService;
use crate::passwords::usage::EntryUsage;
use crate::remote_storage::credentials::{candidates_in, entry_for, state_in_with};
use crate::remote_storage::model::{CredentialsCandidate, CredentialsEntry};
use crate::remote_storage::CredentialsState;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The holzi function that owns the entries the form creates (`haex_passwords_item_details.owner`)
/// and the name the delete warning of the password manager shows.
pub const FEATURE: &str = "marketplace";

/// The caller this module reads and writes the password manager as.
pub const CALLER: Caller = Caller::Internal { feature: FEATURE };

/// The tag of the entries that hold the login of a marketplace (FR-026); the one place of its
/// name.
pub const TAG: &str = "haex-marketplace";

/// What a marketplace's login comes to when it is looked up.
#[derive(Debug, Clone)]
pub enum Login {
    /// The header to send.
    Ready(AuthHeader),
    /// The entry was deleted or lacks the fields of the auth type.
    Missing,
    /// The entry has not arrived through the sync yet.
    Syncing,
}

/// The fixed grant of this module.
fn grants() -> [Grant; 1] {
    [Grant::new(GrantAction::ReadWrite, Scope::All)]
}

/// The header an entry makes for `auth_type` (see [`header_from`]); `None` when it does not fit.
pub fn header_of(item: &SecretItem, auth_type: MarketplaceAuthType) -> Option<AuthHeader> {
    header_from(
        item.username.as_deref(),
        item.password.as_deref(),
        auth_type,
    )
}

/// The header of a login as typed in the form, for the test before anything is saved. The same
/// rule as [`header_of`], so a login that tests fine also works once it is an entry.
pub fn header_of_typed(
    typed: &MarketplaceNewCredentials,
    auth_type: MarketplaceAuthType,
) -> Option<AuthHeader> {
    header_from(typed.username.as_deref(), Some(&typed.secret), auth_type)
}

/// The one rule of "fits" (spec 052): a bearer login takes the password field as the token, a basic
/// login the user name and the password; empty values count as missing.
fn header_from(
    username: Option<&str>,
    password: Option<&str>,
    auth_type: MarketplaceAuthType,
) -> Option<AuthHeader> {
    let password = password.filter(|value| !value.is_empty());
    let username = username.filter(|value| !value.is_empty());
    match (auth_type, username, password) {
        (MarketplaceAuthType::Bearer, _, Some(token)) => Some(AuthHeader::bearer(token)),
        (MarketplaceAuthType::Basic, Some(user), Some(password)) => {
            Some(AuthHeader::basic(user, password))
        }
        _ => None,
    }
}

/// Reads the login the entry `item_id` holds for `auth_type`.
pub async fn read(
    passwords: &PasswordsService,
    db: &VaultDb,
    auth_type: MarketplaceAuthType,
    item_id: &str,
) -> Result<Login> {
    match passwords
        .read_secret_item(&CALLER, &grants(), item_id.to_owned())
        .await
    {
        Ok(item) => Ok(header_of(&item, auth_type).map_or(Login::Missing, Login::Ready)),
        Err(HolziError::PasswordsNotFound) => Ok(match state(db, auth_type, item_id).await? {
            MarketplaceCredentialsState::Syncing => Login::Syncing,
            _ => Login::Missing,
        }),
        // A damaged or unreadable credential must make this one source unavailable, not abort
        // the catalog request for every marketplace. Keep the underlying error out of the UI.
        Err(error) => {
            log::warn!("marketplace credential {item_id} could not be read: {error}");
            Ok(Login::Missing)
        }
    }
}

/// Creates the entry for a login typed in the form and returns its id: "Marketplace: <name>",
/// tagged, owned by `marketplace`; the token or password is a secret field of the manager.
pub async fn create(
    passwords: &PasswordsService,
    name: &str,
    base_url: &str,
    auth_type: MarketplaceAuthType,
    typed: &MarketplaceNewCredentials,
) -> Result<String> {
    if header_of_typed(typed, auth_type).is_none() {
        return Err(HolziError::InvalidInput {
            reason: "marketplace credentials".to_owned(),
        });
    }
    let username = match auth_type {
        MarketplaceAuthType::Basic => typed.username.clone(),
        _ => None,
    };
    let input = ItemInput {
        title: Some(format!("Marketplace: {}", name.trim())),
        username,
        password: Some(typed.secret.to_string()),
        url: Some(base_url.to_owned()),
        tags: vec![TAG.to_owned()],
        ..ItemInput::default()
    };
    passwords.create_owned_item(&CALLER, input).await
}

/// Adds the tag to an entry the user chose, without changing any other tag, its owner or its
/// shares (FR-026).
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

/// Deletes an entry the form created, for good; an entry of the user, or one that is already
/// gone, is left alone.
pub async fn delete(passwords: &PasswordsService, item_id: &str) -> Result<()> {
    match passwords
        .delete_owned_item(&CALLER, item_id.to_owned())
        .await
    {
        Ok(()) | Err(HolziError::PasswordsNotFound) => Ok(()),
        Err(error) => Err(error),
    }
}

/// The entries the user may choose for `auth_type` (research R11): not in the trash, the user's
/// own or created by this module, and fitting by [`header_of`]. Tagged ones first, then by title
/// regardless of case. No value leaves this function.
pub async fn candidates(
    db: &VaultDb,
    auth_type: MarketplaceAuthType,
) -> Result<Vec<CredentialsCandidate>> {
    if !auth_type.needs_credentials() {
        return Ok(Vec::new());
    }
    db.read(move |q| {
        candidates_in(
            q,
            FEATURE,
            TAG,
            |header| {
                // A bearer token needs no user name; both kinds need a password.
                header.has_password
                    && (auth_type != MarketplaceAuthType::Basic
                        || header
                            .username
                            .as_deref()
                            .is_some_and(|name| !name.is_empty()))
            },
            |item| header_of(item, auth_type).is_some(),
        )
        .map_err(Into::into)
    })
    .await
}

/// Whether the entry `item_id` is here with the fields `auth_type` needs, deleted or without
/// them, or not yet arrived.
pub fn state_in(
    q: &mut impl Query,
    auth_type: MarketplaceAuthType,
    item_id: &str,
) -> Result<MarketplaceCredentialsState> {
    Ok(
        match state_in_with(q, item_id, |item| header_of(item, auth_type).is_some())? {
            CredentialsState::Present => MarketplaceCredentialsState::Present,
            CredentialsState::Missing => MarketplaceCredentialsState::Missing,
            CredentialsState::Syncing => MarketplaceCredentialsState::Syncing,
        },
    )
}

/// [`state_in`] on its own read.
pub async fn state(
    db: &VaultDb,
    auth_type: MarketplaceAuthType,
    item_id: &str,
) -> Result<MarketplaceCredentialsState> {
    let item_id = item_id.to_owned();
    db.read(move |q| state_in(q, auth_type, &item_id).map_err(Into::into))
        .await
}

/// The safe display metadata of the entry, if it is still visible.
pub fn entry(q: &mut impl Query, item_id: &str) -> Result<Option<CredentialsEntry>> {
    entry_for(q, FEATURE, item_id)
}

/// Tells the password manager that a marketplace uses an entry (spec 034 FR-034), so deleting
/// "Marketplace: …" there warns first. It reads the vault that is open when it is asked.
pub struct MarketplaceUsage<F> {
    database: F,
}

impl<F> MarketplaceUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    /// Creates a usage check that obtains the active vault from `database` on each request.
    pub fn new(database: F) -> Self {
        Self { database }
    }
}

impl<F> EntryUsage for MarketplaceUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    /// Identifies the marketplace as the feature using the password entry.
    fn feature(&self) -> &'static str {
        FEATURE
    }

    /// Whether a marketplace points at the entry; false if the vault is unavailable or the lookup
    /// fails.
    fn uses(&self, item_id: &str) -> bool {
        let Some(db) = (self.database)() else {
            return false;
        };
        db.read_blocking(|q| super::store::credentials_in_use(q, item_id).map_err(Into::into))
            .unwrap_or(false)
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
