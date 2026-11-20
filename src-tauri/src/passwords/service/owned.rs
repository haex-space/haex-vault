//! Entries that belong to a haex-vault function (spec 038, rule Z14), such as the credentials of a
//! storage connection: only that function creates them and deletes them for good, and nobody but
//! the user and that function reaches them ([`crate::passwords::access::sees_owned`]). No command
//! and no bridge method sets or changes the owner; `create_item` and `update_item` leave it alone,
//! and the user's copy of such an entry keeps it (`copy.rs`). Deleting one for good does not turn
//! the user's placeholders that point at it into own values (FR-048): they become "missing".

use haex_crdt::rusqlite::params;

use super::PasswordsService;
use crate::error::{HaexError, Result};
use crate::passwords::access::Caller;
use crate::passwords::model::ItemInput;
use crate::passwords::{items, trash};

/// The haex-vault function behind `caller`; every other caller is `PasswordsForbidden`.
fn feature_of(caller: &Caller) -> Result<&'static str> {
    match caller {
        Caller::Internal { feature } => Ok(feature),
        _ => Err(HaexError::PasswordsForbidden),
    }
}

impl PasswordsService {
    /// Creates an entry outside every folder that belongs to the calling haex-vault function.
    pub async fn create_owned_item(&self, caller: &Caller, input: ItemInput) -> Result<String> {
        let feature = feature_of(caller)?;
        self.db()
            .write(move |tx| {
                let id = items::create_item(tx, &input, None)?;
                tx.execute(
                    "UPDATE haex_passwords_item_details SET owner = ?2 WHERE id = ?1",
                    params![id, feature],
                )?;
                Ok(id)
            })
            .await
    }

    /// Deletes an entry of the calling haex-vault function for good, without the trash: an exception to
    /// Z11 for its own entries, so removing a storage connection leaves no credentials behind. An
    /// entry of anybody else, or a missing one, is `PasswordsNotFound`.
    pub async fn delete_owned_item(&self, caller: &Caller, item_id: String) -> Result<()> {
        let feature = feature_of(caller)?;
        self.db()
            .write(move |tx| match items::item_state(tx, &item_id)? {
                Some(state) if state.owner.as_deref() == Some(feature) => {
                    trash::purge_item(tx, &item_id)?;
                    Ok(())
                }
                _ => Err(HaexError::PasswordsNotFound.into()),
            })
            .await
    }
}
