//! SQL of migration `0061_passwords_pass` (spec 092-wallet-passes, data-model.md, research R1): the
//! second entry type. `haex_passwords_item_details` gets a `kind` (`login` for every entry that
//! exists), a pass has one row in `haex_passwords_pass_details` and any number of codes in
//! `haex_passwords_pass_barcodes`. The satellites keyed by the item id (free fields, folders, tags,
//! attachments, history) serve both types unchanged.
//!
//! The tables are CRDT-tracked, so [`super::migrations::HOLZI_TRIGGER_VERSION`] is raised with it.
//! No UNIQUE constraint besides the primary keys and no `WITHOUT ROWID` (spec 034 FR-037). The
//! foreign keys cascade locally; `trash::purge_item` still deletes the rows first, since the sync
//! applies remote deletes without foreign keys. `parent_id` has **no** foreign key: the parent may
//! not have arrived by sync yet, and a purge clears the link explicitly (research R8).

/// The migration `0061_passwords_pass`; registered in [`super::migrations::holzi_migration_source`].
pub const PASSWORDS_PASS_0061: &str = r#"ALTER TABLE haex_passwords_item_details ADD COLUMN kind TEXT NOT NULL DEFAULT 'login';
--> statement-breakpoint
CREATE TABLE haex_passwords_pass_details (
  item_id TEXT PRIMARY KEY NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  pass_kind TEXT NOT NULL DEFAULT 'other',
  start_date TEXT,
  start_at TEXT,
  start_zone TEXT,
  parent_id TEXT,
  source_id TEXT
);
--> statement-breakpoint
CREATE INDEX idx_haex_passwords_pass_details_parent_id ON haex_passwords_pass_details (parent_id);
--> statement-breakpoint
CREATE TABLE haex_passwords_pass_barcodes (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  format TEXT NOT NULL,
  value TEXT NOT NULL,
  label TEXT,
  position INTEGER NOT NULL DEFAULT 0,
  flag TEXT
);
--> statement-breakpoint
CREATE INDEX idx_haex_passwords_pass_barcodes_item_id ON haex_passwords_pass_barcodes (item_id);"#;
