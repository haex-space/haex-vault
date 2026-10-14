//! Derived ids of the address book (spec 051, data-model.md). The synced tables have no UNIQUE
//! constraint (a conflict would halt the sync); where two devices may create the same row, its id
//! is a UUIDv5 over the natural key, so both write the same row and merge.

use uuid::Uuid;

/// Namespace of every derived id of the address book; the kind of row is part of the name.
/// Generated once; **never change** it, or old and new devices would derive different ids.
pub const NS_CONTACTS: Uuid = Uuid::from_u128(0x5c0f_7a2e_9d41_4b8a_a6e3_1f2d_0c9b_8e47);

fn derive(kind: &str, parts: &[&str]) -> String {
    let name = std::iter::once(kind)
        .chain(parts.iter().copied())
        .collect::<Vec<_>>()
        .join(":");
    Uuid::new_v5(&NS_CONTACTS, name.as_bytes()).to_string()
}

/// `haex_contacts_targets.id`: one sync target per contact and phone.
pub fn target_id(contact_id: &str, vault_device_uuid: &str) -> String {
    derive("target", &[contact_id, vault_device_uuid])
}

/// `haex_contacts_phone_links.id`: one row per phone and raw contact of that phone.
pub fn link_id(vault_device_uuid: &str, raw_contact_id: i64) -> String {
    derive("link", &[vault_device_uuid, &raw_contact_id.to_string()])
}

/// `haex_contacts_group_members.id`: a contact is in a group once.
pub fn member_id(group_id: &str, contact_id: &str) -> String {
    derive("member", &[group_id, contact_id])
}

/// `haex_contacts_phone_issues.id`: one open issue per phone and contact.
pub fn issue_id(vault_device_uuid: &str, contact_id: &str) -> String {
    derive("issue", &[vault_device_uuid, contact_id])
}

/// An uncertain pair of the first run: a contact of the phone (its aggregated contact's lookup
/// key) and a contact of the address book. Stable between the overview and the confirmation.
pub fn pair_id(group_key: &str, contact_id: &str) -> String {
    derive("pair", &[group_key, contact_id])
}

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
