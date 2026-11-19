//! The ratchet account of this installation (data-model.md "Ratchet account"): its ratchet identity
//! key and its start keys (Olm fallback keys). Created on first use, so a vault file copied to
//! another installation gets an account of its own there.
//!
//! A new start key replaces the current one, which stays usable until the next renewal: an opening
//! message made for it still creates a session for one renewal period after it stopped being
//! current, and no longer after two renewals.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;
use vodozemac::olm::{Account, AccountPickle};
use vodozemac::Curve25519PublicKey;
use zeroize::Zeroizing;

use super::store::{self, AccountRow};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// The public keys a chat key record carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnKeys {
    /// The ratchet identity key (Curve25519).
    pub identity: Curve25519PublicKey,
    /// The current start key.
    pub start: Curve25519PublicKey,
}

/// The account of `installation` and when its current start key was made; `None` before the first
/// [`load_or_create`].
pub fn load(q: &mut impl Query, installation: Uuid) -> Result<Option<(Account, i64)>> {
    let Some(row) = store::account(q, installation)? else {
        return Ok(None);
    };
    Ok(Some((unpickle(&row.account)?, row.start_key_made_at)))
}

/// The account of `installation`; a new one with one start key made at `now` when it has none.
pub fn load_or_create(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    now: i64,
) -> Result<Account> {
    if let Some((account, _)) = load(tx, installation)? {
        return Ok(account);
    }
    let mut account = Account::new();
    account.generate_fallback_key();
    store::put_account(
        tx,
        installation,
        &AccountRow {
            account: pickle(&account)?,
            start_key_made_at: now,
            record_published_at: None,
        },
    )?;
    Ok(account)
}

/// The public keys of `account`.
pub fn own_keys(account: &Account) -> Result<OwnKeys> {
    let start = account
        .fallback_key()
        .into_values()
        .next()
        .ok_or_else(missing)?;
    Ok(OwnKeys {
        identity: account.curve25519_key(),
        start,
    })
}

/// Makes a new start key at `now`; the account then holds it and the one before, and forgets the
/// one before that. The caller publishes the new record.
pub fn renew_start_key(tx: &mut CrdtTransaction<'_>, installation: Uuid, now: i64) -> Result<()> {
    let Some(mut row) = store::account(tx, installation)? else {
        return Err(missing());
    };
    let mut account = unpickle(&row.account)?;
    account.generate_fallback_key();
    row.account = pickle(&account)?;
    row.start_key_made_at = now;
    row.record_published_at = None;
    store::put_account(tx, installation, &row)
}

fn pickle(account: &Account) -> Result<Zeroizing<String>> {
    serde_json::to_string(&account.pickle())
        .map(Zeroizing::new)
        .map_err(|_| broken("pickling"))
}

fn unpickle(text: &str) -> Result<Account> {
    let pickle: AccountPickle = serde_json::from_str(text).map_err(|_| broken("reading"))?;
    Ok(Account::from_pickle(pickle))
}

fn missing() -> HaexError {
    HaexError::NotFound {
        name: "chat ratchet account".into(),
    }
}

/// The error of a pickle that does not read or write; never its content.
fn broken(what: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: format!("chat ratchet account: {what} failed"),
    }
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
