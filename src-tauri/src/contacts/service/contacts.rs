//! Contact methods of the service (spec 051 FR-001 to FR-007, FR-018a, FR-028 to FR-031): list,
//! read, create, change, delete and the photo. Every caller but the user and holzi's own functions
//! reaches only what its grants cover; a contact outside the scope answers like a missing one.

use std::collections::BTreeSet;

use uuid::Uuid;

use super::{caller_text, Caller, ContactsService, Grant};
use crate::contacts::access::{
    authorize_create, authorize_list, authorize_read, authorize_write, require_user,
};
use crate::contacts::kinds::{self, EMAIL, PHONE};
use crate::contacts::model::{
    Contact, ContactHeader, ContactInput, ContactPatch, Head, Origin, OriginKind, ValueInput,
};
use crate::contacts::{photos, store};
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::patch::Patch;

/// Most characters of a head field; the note may be longer.
const MAX_FIELD_CHARS: usize = 1024;

/// Most bytes of a note.
pub const MAX_NOTE_BYTES: usize = 64 * 1024;

fn invalid(reason: &str) -> HolziError {
    HolziError::ContactsInvalid {
        reason: reason.to_owned(),
    }
}

fn check_field(column: &str, value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let too_long = if column == "note" {
        value.len() > MAX_NOTE_BYTES
    } else {
        value.chars().count() > MAX_FIELD_CHARS
    };
    if too_long {
        Err(invalid(&format!("{column} too long")))
    } else {
        Ok(())
    }
}

fn head_fields(head: &Head) -> [(&'static str, Option<&str>); 10] {
    [
        ("display_name", head.display_name.as_deref()),
        ("given_name", head.given_name.as_deref()),
        ("family_name", head.family_name.as_deref()),
        ("middle_name", head.middle_name.as_deref()),
        ("name_prefix", head.name_prefix.as_deref()),
        ("name_suffix", head.name_suffix.as_deref()),
        ("nickname", head.nickname.as_deref()),
        ("organization", head.organization.as_deref()),
        ("job_title", head.job_title.as_deref()),
        ("note", head.note.as_deref()),
    ]
}

fn check_value(input: &ValueInput) -> Result<()> {
    kinds::validate(&input.kind, &input.label, &input.value).map_err(|reason| invalid(&reason))
}

fn no_identity() -> HolziError {
    invalid("a contact needs a name, an organization, a phone number or an e-mail address")
}

/// Fails unless the stored contact has something to be found by (after a write, in the same
/// transaction, so a refused change writes nothing).
fn check_identity(q: &mut impl crate::storage::query::Query, contact_id: &str) -> Result<()> {
    let contact = store::load(q, contact_id)?;
    let count = |kind: &str| contact.values.iter().filter(|v| v.kind == kind).count();
    if store::has_identity(&contact.head, count(PHONE), count(EMAIL)) {
        Ok(())
    } else {
        Err(no_identity())
    }
}

impl ContactsService {
    /// The headers of the contacts the caller may see.
    pub async fn list(&self, caller: &Caller, grants: &[Grant]) -> Result<Vec<ContactHeader>> {
        let scope = authorize_list(caller, grants)?;
        let mut headers = self
            .db()
            .read(|q| store::headers(q).map_err(Into::into))
            .await?;
        headers.retain(|header| scope.covers(&header.groups));
        Ok(headers)
    }

    /// One contact; outside the caller's scope it is `ContactsNotFound`, like a missing one.
    pub async fn get(&self, caller: &Caller, grants: &[Grant], id: &str) -> Result<Contact> {
        let id = id.to_owned();
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .read(move |q| {
                let groups =
                    store::group_ids_if_exists(q, &id)?.ok_or(HolziError::ContactsNotFound)?;
                authorize_read(&caller, &grants, &groups).map_err(HolziError::from)?;
                Ok(store::load(q, &id)?)
            })
            .await
    }

    /// Creates a contact made in holzi: its origin is this device and the caller (FR-018a).
    pub async fn create(
        &self,
        caller: &Caller,
        grants: &[Grant],
        input: ContactInput,
    ) -> Result<Contact> {
        let groups: BTreeSet<String> = input.groups.iter().cloned().collect();
        authorize_create(caller, grants, &input.groups)?;
        for (column, value) in head_fields(&input.head) {
            check_field(column, value)?;
        }
        for value in &input.values {
            check_value(value)?;
        }
        let count = |kind: &str| input.values.iter().filter(|v| v.kind == kind).count();
        if !store::has_identity(&input.head, count(PHONE), count(EMAIL)) {
            return Err(no_identity());
        }
        let origin = Origin {
            kind: OriginKind::Holzi,
            device: self.device(),
            caller: caller_text(caller),
            account: None,
        };
        let id = Uuid::new_v4().to_string();
        self.db()
            .write(move |tx| {
                store::check_groups_exist(tx, &groups)?;
                store::insert(tx, &id, &input.head, &origin, &clock::now())?;
                for value in &input.values {
                    let position = store::next_position(tx, &id, &value.kind)?;
                    store::insert_value(
                        tx,
                        &id,
                        &value.kind,
                        &value.label,
                        &value.value,
                        position,
                    )?;
                }
                store::set_groups(tx, &id, &groups)?;
                Ok(store::load(tx, &id)?)
            })
            .await
    }

    /// Changes a contact: only the head fields and value rows the patch names are written; values
    /// of kinds this version does not know are read-only (FR-007); the origin never changes.
    pub async fn update(
        &self,
        caller: &Caller,
        grants: &[Grant],
        id: &str,
        patch: ContactPatch,
    ) -> Result<Contact> {
        for (column, value) in patch.head_columns() {
            if let Patch::Set(text) = value {
                check_field(column, Some(text))?;
            }
        }
        let values = patch.values.clone().unwrap_or_default();
        for upsert in &values.upsert {
            check_value(&ValueInput {
                kind: upsert.kind.clone(),
                label: upsert.label.clone(),
                value: upsert.value.clone(),
            })?;
        }
        let id = id.to_owned();
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .write(move |tx| {
                let groups =
                    store::group_ids_if_exists(tx, &id)?.ok_or(HolziError::ContactsNotFound)?;
                authorize_write(&caller, &grants, &groups).map_err(HolziError::from)?;
                for (column, value) in patch.head_columns() {
                    match value {
                        Patch::Keep => {}
                        Patch::Clear => store::set_head(tx, &id, column, None)?,
                        Patch::Set(text) => store::set_head(tx, &id, column, Some(text))?,
                    }
                }
                for value_id in &values.remove {
                    let kind = owned_value_kind(tx, &id, value_id)?;
                    if !kinds::is_known(&kind) {
                        return Err(HolziError::ContactsUnknownKindReadOnly.into());
                    }
                    store::delete_value(tx, value_id)?;
                }
                for upsert in &values.upsert {
                    match &upsert.id {
                        Some(value_id) => {
                            let kind = owned_value_kind(tx, &id, value_id)?;
                            if !kinds::is_known(&kind) {
                                return Err(HolziError::ContactsUnknownKindReadOnly.into());
                            }
                            if kind != upsert.kind {
                                return Err(invalid("the kind of a value cannot change").into());
                            }
                            store::update_value(
                                tx,
                                value_id,
                                &upsert.label,
                                &upsert.value,
                                upsert.position,
                            )?;
                        }
                        None => {
                            let position = match upsert.position {
                                Some(position) => position,
                                None => store::next_position(tx, &id, &upsert.kind)?,
                            };
                            store::insert_value(
                                tx,
                                &id,
                                &upsert.kind,
                                &upsert.label,
                                &upsert.value,
                                position,
                            )?;
                        }
                    }
                }
                if let Some(new_groups) = &patch.groups {
                    let new_groups: BTreeSet<String> = new_groups.iter().cloned().collect();
                    store::check_groups_exist(tx, &new_groups)?;
                    // A caller with a group scope keeps the contact inside it.
                    let listed: Vec<String> = new_groups.iter().cloned().collect();
                    authorize_write(&caller, &grants, &listed).map_err(HolziError::from)?;
                    store::set_groups(tx, &id, &new_groups)?;
                }
                check_identity(tx, &id)?;
                store::touch(tx, &id, &clock::now())?;
                Ok(store::load(tx, &id)?)
            })
            .await
    }

    /// Deletes contacts; all or nothing.
    pub async fn delete(&self, caller: &Caller, grants: &[Grant], ids: Vec<String>) -> Result<()> {
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .write(move |tx| {
                for id in &ids {
                    let groups =
                        store::group_ids_if_exists(tx, id)?.ok_or(HolziError::ContactsNotFound)?;
                    authorize_write(&caller, &grants, &groups).map_err(HolziError::from)?;
                    let photo = store::photo_of(tx, id)?;
                    store::delete(tx, id)?;
                    if let Some(hash) = photo {
                        photos::mark_if_unused(tx, &hash)?;
                    }
                }
                Ok(())
            })
            .await
    }

    /// Sets (or with empty `bytes` removes) the photo of a contact; the image is shrunk first
    /// (FR-005). For the window only.
    pub async fn set_photo(&self, caller: &Caller, id: &str, bytes: Vec<u8>) -> Result<Contact> {
        require_user(caller)?;
        let jpeg = if bytes.is_empty() {
            None
        } else {
            Some(
                tauri::async_runtime::spawn_blocking(move || photos::shrink(&bytes))
                    .await
                    .map_err(|e| invalid(&format!("photo: {e}")))??,
            )
        };
        let id = id.to_owned();
        self.db()
            .write(move |tx| {
                store::group_ids_if_exists(tx, &id)?.ok_or(HolziError::ContactsNotFound)?;
                let old = store::photo_of(tx, &id)?;
                let new = match &jpeg {
                    Some(jpeg) => Some(photos::store(tx, jpeg)?),
                    None => None,
                };
                store::set_photo(tx, &id, new.as_deref())?;
                if let Some(old) = old.filter(|old| Some(old) != new.as_ref()) {
                    photos::mark_if_unused(tx, &old)?;
                }
                store::touch(tx, &id, &clock::now())?;
                Ok(store::load(tx, &id)?)
            })
            .await
    }

    /// The JPEG of a photo that a contact the caller may read uses.
    pub async fn photo(&self, caller: &Caller, grants: &[Grant], hash: &str) -> Result<Vec<u8>> {
        let scope = authorize_list(caller, grants)?;
        let hash = hash.to_owned();
        self.db()
            .read(move |q| {
                let visible = store::headers(q)?.into_iter().any(|header| {
                    header.photo.as_deref() == Some(hash.as_str()) && scope.covers(&header.groups)
                });
                if !visible {
                    return Err(HolziError::ContactsNotFound.into());
                }
                Ok(photos::data(q, &hash)?)
            })
            .await
    }
}

/// The kind of a value that must belong to `contact_id`; another contact's value is not found.
fn owned_value_kind(
    q: &mut impl crate::storage::query::Query,
    contact_id: &str,
    value_id: &str,
) -> Result<String> {
    match store::value_owner(q, value_id)? {
        Some((owner, kind)) if owner == contact_id => Ok(kind),
        _ => Err(HolziError::ContactsNotFound),
    }
}

#[cfg(test)]
#[path = "contacts_tests.rs"]
mod tests;
