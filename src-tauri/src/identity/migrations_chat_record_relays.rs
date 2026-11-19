//! SQL of migration `0066_chat_record_relays` (spec 095 T013): the Nostr-Relays the own chat key
//! record was last published to, so a device publishes it again when its relays change and not on
//! every open, which would show the relays when the vault opens. A `_no_sync` table changes, so
//! [`super::migrations::HAEX_TRIGGER_VERSION`] stays.

/// The migration `0066_chat_record_relays`; registered in
/// [`super::migrations::haex_migration_source`].
pub const CHAT_RECORD_RELAYS_0066: &str =
    "ALTER TABLE chat_ratchet_account_no_sync ADD COLUMN record_relays TEXT NOT NULL DEFAULT '[]';";
