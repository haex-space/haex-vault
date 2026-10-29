//! SQL of migration `0048_marketplaces` (spec 069, data-model.md, research R10, R11, R14): the
//! marketplaces a vault asks for extensions. Every row is equal; the public marketplace of the
//! project is only a preset row with the fixed id `preset-public`.
//!
//! The table carries no secret: a marketplace that needs a login points at an entry of the
//! password manager (`credentials_item_id`), which has no foreign key because the entry may arrive
//! through the sync after the row. It has no UNIQUE constraint either (a conflict would halt the
//! sync, as in [`super::migrations_browser_clients`]): the order is `(sort_order, name, id)` when
//! two rows tie. New CRDT tables raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0048_marketplaces`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const MARKETPLACES_0048: &str = r#"CREATE TABLE haex_marketplaces (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL,
  auth_type TEXT NOT NULL DEFAULT 'none',
  credentials_item_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);"#;

/// The tables of [`MARKETPLACES_0048`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 1] = ["haex_marketplaces"];
