//! SQL of migration `0035_file_icon_sets` (spec 053, data-model.md, research R3): the custom icon
//! sets of the file browser. A set is one row; its pack (a ZIP with the manifest, the sanitized
//! images and the license) is the BLOB of that row, so the sync brings a set whole or not at all.
//!
//! The table is vault-wide and syncs. It carries no UNIQUE constraint (a conflict would halt the
//! sync, as in [`super::migrations_storage`]); that a name occurs once and every limit of a pack
//! hold is checked in `files::icons`. A set never changes: replacing one is a new row and the
//! delete of the old. New CRDT tables raise [`super::migrations::HOLZI_TRIGGER_VERSION`].
//!
//! Number: `0035` was the next free one on `main` when this was written (the last is
//! `0034_contacts_phone_known`).

/// The migration `0035_file_icon_sets`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const FILE_ICON_SETS_0035: &str = r#"CREATE TABLE haex_file_icon_sets (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  author TEXT NOT NULL DEFAULT '',
  license TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT '',
  image_count INTEGER NOT NULL,
  byte_size INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  pack BLOB NOT NULL,
  created_at TEXT NOT NULL
);"#;

/// The tables of [`FILE_ICON_SETS_0035`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 1] = ["haex_file_icon_sets"];
