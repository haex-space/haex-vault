//! SQL of migration `0065_haex_names`: the app was renamed (spec 054), and the names it stored in a
//! vault follow. The maintenance table gets its new name, and the contacts made in the app and the
//! addresses of the app on a contact carry the new kind. Vaults created after the rename have the
//! old names from the older migrations first and the new ones after this one, so every vault ends
//! the same.
//!
//! The older migrations keep their text: haex-crdt compares each applied migration by its content,
//! so a changed text would make a vault from before the rename unreadable. For the same reason this
//! file is the one place that still names the old spelling.
//!
//! The maintenance table is `_no_sync` (no CRDT triggers). The kinds are data of tracked tables, as
//! in the data-only migration `0041_contacts_card_display_name`, so
//! [`super::migrations::HAEX_TRIGGER_VERSION`] stays.

/// The migration `0065_haex_names`; registered in [`super::migrations::haex_migration_source`].
pub const HAEX_NAMES_0065: &str = r#"ALTER TABLE holzi_maintenance_no_sync RENAME TO haex_maintenance_no_sync;
--> statement-breakpoint
UPDATE haex_contacts SET origin_kind = 'haex' WHERE origin_kind = 'holzi';
--> statement-breakpoint
UPDATE haex_contacts_values SET kind = 'haex_vault' WHERE kind = 'holzi_vault';
--> statement-breakpoint
UPDATE haex_contacts_values SET kind = 'haex_nostr_relay' WHERE kind = 'holzi_nostr_relay';
--> statement-breakpoint
UPDATE haex_contacts_values SET kind = 'haex_iroh' WHERE kind = 'holzi_iroh';"#;
