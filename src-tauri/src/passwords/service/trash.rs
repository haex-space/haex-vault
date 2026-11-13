//! Delete and trash methods of the service (spec 034, US4, FR-015, FR-028, rules Z8, Z11, Z13).
//!
//! `delete_item` is open to every caller whose grant covers the entry and **moves it into the
//! trash**, never removes it: it only runs for an entry outside the trash, because `trash::trash`
//! on an entry that is in the trash would delete it for good. For every caller but the user an
//! entry in the trash does not exist (Z13), so a second delete from outside cannot reach that
//! branch. Restoring, deleting for good and emptying the trash are for the user alone (Z11).

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::passwords::access::{authorize_delete, Caller, Grant, ItemState};
use crate::passwords::model::{Target, TargetKind};
use crate::passwords::{items, references_db, trash};

impl PasswordsService {
    /// Moves an entry into the trash (Z8). A caller needs a write grant and an entry in its scope;
    /// the user needs nothing, and an entry that is in the trash already is `PasswordsNotFound`
    /// here (the window removes it for good through [`Self::trash_targets`]).
    pub async fn delete_item(
        &self,
        caller: &Caller,
        grants: &[Grant],
        item_id: String,
    ) -> Result<()> {
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .write(move |tx| {
                let Some(state) = items::item_state(tx, &item_id)? else {
                    // Forbidden without a write grant, otherwise not found (Z3, Z5).
                    authorize_delete(
                        &caller,
                        &grants,
                        &ItemState {
                            tags: &[],
                            in_trash: false,
                            owner: None,
                            login: true,
                        },
                    )
                    .map_err(HolziError::from)?;
                    return Err(HolziError::PasswordsNotFound.into());
                };
                authorize_delete(&caller, &grants, &state.view()).map_err(HolziError::from)?;
                if state.in_trash {
                    // Only the user reaches this far (Z13); a delete never removes for good.
                    return Err(HolziError::PasswordsNotFound.into());
                }
                trash::trash(
                    tx,
                    &[Target {
                        kind: TargetKind::Item,
                        id: item_id,
                    }],
                )
                .map(|_| ())
                .map_err(Into::into)
            })
            .await
    }

    /// Moves entries and folders into the trash; what is in the trash already is deleted for good
    /// (the window's "Delete" in the trash). Returns the number affected. Z11.
    pub async fn trash_targets(&self, caller: &Caller, targets: Vec<Target>) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| trash::trash(tx, &targets).map_err(Into::into))
            .await
    }

    /// Takes entries and folders out of the trash. Z11.
    pub async fn restore(&self, caller: &Caller, targets: Vec<Target>) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| trash::restore(tx, &targets).map_err(Into::into))
            .await
    }

    /// Deletes what is in the trash for good, with everything that depends on it. With
    /// `inline_references` the placeholders other entries hold on the deleted entries are first
    /// replaced by today's values, in the same transaction (spec 036, FR-048). Z11.
    pub async fn delete_permanently(
        &self,
        caller: &Caller,
        targets: Vec<Target>,
        inline_references: bool,
    ) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                if inline_references {
                    let mut sources: Vec<String> = targets
                        .iter()
                        .filter(|t| t.kind == TargetKind::Item)
                        .map(|t| t.id.clone())
                        .collect();
                    let groups: Vec<String> = targets
                        .iter()
                        .filter(|t| t.kind == TargetKind::Group)
                        .map(|t| t.id.clone())
                        .collect();
                    sources.extend(references_db::items_in_groups(tx, &groups)?);
                    references_db::inline_all(tx, &sources)?;
                }
                trash::delete_permanently(tx, &targets).map_err(Into::into)
            })
            .await
    }

    /// Empties the trash, with `inline_references` as in [`Self::delete_permanently`]. Z11.
    pub async fn empty_trash(&self, caller: &Caller, inline_references: bool) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                if inline_references {
                    let sources: Vec<String> = items::trashed_item_ids(tx)?.into_iter().collect();
                    references_db::inline_all(tx, &sources)?;
                }
                trash::empty_trash(tx).map_err(Into::into)
            })
            .await
    }
}
