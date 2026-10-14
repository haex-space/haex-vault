//! Group methods of the service (spec 051 FR-003): the window lists, creates, renames and fills
//! groups. Groups are the scope of grants (FR-029, FR-030), so only the user manages them.

use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::{Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::ids::member_id;
use crate::contacts::model::Group;
use crate::contacts::store;
use crate::error::{HolziError, Result};

/// Most characters of a group name.
const MAX_NAME_CHARS: usize = 100;

impl ContactsService {
    /// Every group.
    pub async fn groups(&self, caller: &Caller) -> Result<Vec<Group>> {
        require_user(caller)?;
        self.db()
            .read(|q| store::all_groups(q).map_err(Into::into))
            .await
    }

    /// Creates a group (`id` absent) or renames one.
    pub async fn group_save(
        &self,
        caller: &Caller,
        id: Option<String>,
        name: String,
    ) -> Result<Group> {
        require_user(caller)?;
        let name = name.trim().to_owned();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
            return Err(HolziError::ContactsInvalid {
                reason: "invalid group name".to_owned(),
            });
        }
        self.db()
            .write(move |tx| {
                let id = match id {
                    Some(id) => {
                        let changed = tx.execute(
                            "UPDATE haex_contacts_groups SET name = ?1 WHERE id = ?2",
                            params![name, id],
                        )?;
                        if changed == 0 {
                            return Err(HolziError::ContactsNotFound.into());
                        }
                        id
                    }
                    None => {
                        let id = Uuid::new_v4().to_string();
                        tx.execute(
                            "INSERT INTO haex_contacts_groups (id, name) VALUES (?1, ?2)",
                            params![id, name],
                        )?;
                        id
                    }
                };
                Ok(Group { id, name })
            })
            .await
    }

    /// Deletes a group; its contacts stay.
    pub async fn group_delete(&self, caller: &Caller, id: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                tx.execute(
                    "DELETE FROM haex_contacts_groups WHERE id = ?1",
                    params![id],
                )?;
                Ok(())
            })
            .await
    }

    /// Adds contacts to a group and removes others from it; adding twice changes nothing.
    pub async fn group_members(
        &self,
        caller: &Caller,
        group_id: String,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                store::check_groups_exist(tx, &BTreeSet::from([group_id.clone()]))?;
                for contact in &remove {
                    tx.execute(
                        "DELETE FROM haex_contacts_group_members WHERE id = ?1",
                        params![member_id(&group_id, contact)],
                    )?;
                }
                for contact in &add {
                    store::group_ids_if_exists(tx, contact)?.ok_or(HolziError::ContactsNotFound)?;
                    let mut groups: BTreeSet<String> = store::groups_of(tx, contact)?
                        .into_iter()
                        .map(|group| group.id)
                        .collect();
                    groups.insert(group_id.clone());
                    store::set_groups(tx, contact, &groups)?;
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
