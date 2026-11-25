//! Organizing methods of the service (spec 034, US2): folders, moving, the order of folders, tags.
//! Everything here is for the user alone (rule Z11); renaming and deleting a tag in particular is
//! never open to a caller from outside (FR-028).

use super::{require_user, PasswordsService};
use crate::agent_profiles::tag_references::profiles_naming_tag;
use crate::error::{HaexError, Result};
use crate::passwords::access::Caller;
use crate::passwords::model::{GroupPatch, Target};
use crate::passwords::{groups, tags};

impl PasswordsService {
    pub async fn create_group(
        &self,
        caller: &Caller,
        name: String,
        description: Option<String>,
        icon: Option<String>,
        color: Option<String>,
        parent_id: Option<String>,
    ) -> Result<String> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                groups::create_group(
                    tx,
                    &name,
                    description.as_deref(),
                    icon.as_deref(),
                    color.as_deref(),
                    parent_id.as_deref(),
                )
                .map_err(Into::into)
            })
            .await
    }

    pub async fn update_group(
        &self,
        caller: &Caller,
        group_id: String,
        patch: GroupPatch,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| groups::update_group(tx, &group_id, &patch).map_err(Into::into))
            .await
    }

    pub async fn reorder_groups(
        &self,
        caller: &Caller,
        parent_id: Option<String>,
        ordered_ids: Vec<String>,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                groups::reorder_groups(tx, parent_id.as_deref(), &ordered_ids).map_err(Into::into)
            })
            .await
    }

    /// Moves entries and folders, all or none. Returns how many targets moved.
    pub async fn move_targets(
        &self,
        caller: &Caller,
        targets: Vec<Target>,
        to_group_id: Option<String>,
    ) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                groups::move_targets(tx, &targets, to_group_id.as_deref()).map_err(Into::into)
            })
            .await
    }

    /// Adds and removes tags on many entries; returns how many entries changed.
    pub async fn set_tags(
        &self,
        caller: &Caller,
        item_ids: Vec<String>,
        add: Vec<String>,
        remove: Vec<String>,
    ) -> Result<u32> {
        require_user(caller)?;
        self.db()
            .write(move |tx| tags::bulk_set(tx, &item_ids, &add, &remove).map_err(Into::into))
            .await
    }

    pub async fn rename_tag(&self, caller: &Caller, tag_id: String, name: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| tags::rename_tag(tx, &tag_id, &name).map_err(Into::into))
            .await
    }

    pub async fn set_tag_color(
        &self,
        caller: &Caller,
        tag_id: String,
        color: Option<String>,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| tags::set_color(tx, &tag_id, color.as_deref()).map_err(Into::into))
            .await
    }

    pub async fn delete_tag(&self, caller: &Caller, tag_id: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| tags::delete_tag(tx, &tag_id).map_err(Into::into))
            .await
    }

    /// Merges the tag `source_id` into `target_id` for every entry, the result under `name` when
    /// one is given (spec 100); the user alone.
    pub async fn merge_tags(
        &self,
        caller: &Caller,
        source_id: String,
        target_id: String,
        name: Option<String>,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                tags::merge_tags(tx, &source_id, &target_id, name.as_deref()).map_err(Into::into)
            })
            .await
    }

    /// The names of the agent profiles whose password rights name this tag (spec 100, FR-010), for
    /// the confirmation of a merge. `PasswordsNotFound` for a tag that does not exist.
    pub async fn tag_agent_profiles(&self, tag_id: String) -> Result<Vec<String>> {
        self.db()
            .read(move |q| {
                let name = tags::name_of(q, &tag_id)?.ok_or(HaexError::PasswordsNotFound)?;
                profiles_naming_tag(q, &name)
            })
            .await
    }
}

#[cfg(test)]
#[path = "organize_tests.rs"]
mod tests;
