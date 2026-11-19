//! SQL of migration `0026_passwords_refs` (spec 036-password-redesign, data-model.md, research R6):
//! `haex_passwords_passkey_links`, a passkey shown at another entry by a link instead of a copy of
//! its key. CRDT-tracked like the tables of `0022_passwords`; no UNIQUE constraint, the id is
//! `UUIDv5(NS_PASSKEY_LINK, "item_id:passkey_id")`, so the same link made on two devices is one
//! row. The foreign keys cascade locally; `trash::purge_item` still deletes the links first, since
//! the sync applies remote deletes without foreign keys. References between entries are text in
//! the existing columns and need no table.
//!
//! Spec 036 planned this as `0024`; spec 017 took `0024` and `0025` first.

/// The migration `0026_passwords_refs`; registered in [`super::migrations::haex_migration_source`].
pub const PASSWORDS_REFS_0026: &str = r#"CREATE TABLE haex_passwords_passkey_links (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE,
  passkey_id TEXT NOT NULL REFERENCES haex_passwords_passkeys(id) ON DELETE CASCADE,
  created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE INDEX idx_haex_passwords_passkey_links_item_id ON haex_passwords_passkey_links (item_id);
--> statement-breakpoint
CREATE INDEX idx_haex_passwords_passkey_links_passkey_id ON haex_passwords_passkey_links (passkey_id);"#;
