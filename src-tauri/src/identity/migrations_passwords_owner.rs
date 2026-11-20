//! SQL of migration `0027_passwords_owner` (spec 038, data-model.md, research R2): the column `owner`
//! of a password entry. `NULL` is an entry of the user; otherwise it names the haex-vault function the
//! entry belongs to (`storage` for the credentials of a storage connection), and rule Z14 of spec 034
//! hides the entry from every caller but the user and that function. The table is CRDT-tracked, so
//! [`super::migrations::HAEX_TRIGGER_VERSION`] is raised with it.

/// The migration `0027_passwords_owner`; registered in [`super::migrations::haex_migration_source`].
pub const PASSWORDS_OWNER_0027: &str =
    "ALTER TABLE haex_passwords_item_details ADD COLUMN owner TEXT;";
