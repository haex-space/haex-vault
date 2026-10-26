//! SQL of migration `0043_contacts_cards_hash` (spec 060 research R5, data-model.md): the cards of
//! other vaults, several per vault, keyed by the hash of their signed content.
//!
//! The old table (`vault_pubkey` as the primary key) is dropped and made again; no row is copied.
//! The key of a row is the SHA-256 of the card's signed payload, which only Rust can compute, so an
//! old row could not get its key in SQL. **Confirmed holzi addresses of existing contacts go with
//! it and come back when their card is read again**; the contacts stay. `DROP TABLE` fires no
//! triggers, so no delete marker reaches other devices (precedent: `0021_own_device_sync`), and
//! `HOLZI_TRIGGER_VERSION` is raised so the new table gets its triggers on every existing vault.
//!
//! The table has no UNIQUE constraint, which would halt the sync on a conflict: the primary key
//! `hash` makes the same card one row on every device, and the index on `vault_pubkey` is plain.

/// The migration `0043_contacts_cards_hash`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const CONTACTS_CARDS_0043: &str = r#"DROP TABLE haex_contacts_cards;
--> statement-breakpoint
CREATE TABLE haex_contacts_cards (
  hash TEXT PRIMARY KEY NOT NULL,
  vault_pubkey TEXT NOT NULL,
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  card BLOB NOT NULL,
  issued_at TEXT NOT NULL,
  received_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE INDEX haex_contacts_cards_vault ON haex_contacts_cards(vault_pubkey);
--> statement-breakpoint
CREATE INDEX haex_contacts_cards_contact ON haex_contacts_cards(contact_id);
"#;
