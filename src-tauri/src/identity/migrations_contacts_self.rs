//! SQL of migration `0042_contacts_self` (spec 060 FR-001, research R1): the own contact, "Ich".
//!
//! One fixed row with the id `contacts::ids::SELF_CONTACT_ID` and constant values, so every device
//! writes the same row and the rows meet without a conflict. The INSERT names no HLC column:
//! haex-crdt stamps a row a migration wrote before the HLC existed on the next open, and the row
//! syncs like any other (lifecycle step 6 of `Database::open`). It is `INSERT OR IGNORE` because a
//! newer device may have synced the row to this vault before this device ran the migration.

/// The migration `0042_contacts_self`; registered in [`super::migrations::holzi_migration_source`].
/// The id in the SQL is [`crate::contacts::ids::SELF_CONTACT_ID`]; a test keeps the two equal.
pub const CONTACTS_SELF_0042: &str = "INSERT OR IGNORE INTO haex_contacts \
  (id, origin_kind, origin_device, origin_caller, created_at, updated_at) \
  VALUES ('1ce52853-b579-4ffd-b9ef-f27071a414ed', 'holzi', \
  '00000000-0000-0000-0000-000000000000', 'internal:self', \
  '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');";
