//! SQL of migration `0070_chat_gate` (spec 099, data-model.md): a request keeps when its stranger
//! knocked first and last, the sender's side notes that the other vault blocked it, a block entry
//! keeps the Visitenkarte of a declined stranger, and the answers still to send wait on the device.
//!
//! `chat_threads` and `haex_contacts_blocked_vaults` are tracked by haex-crdt, so the new columns
//! raise [`super::migrations::HAEX_TRIGGER_VERSION`]; `chat_gate_notices_no_sync` stays on the device.

/// The migration `0070_chat_gate`; registered in [`super::migrations::haex_migration_source`].
pub const CHAT_GATE_0070: &str = "\
ALTER TABLE chat_threads ADD COLUMN request_first_at INTEGER;
ALTER TABLE chat_threads ADD COLUMN request_last_at INTEGER;
ALTER TABLE chat_threads ADD COLUMN peer_blocked_at INTEGER;
ALTER TABLE haex_contacts_blocked_vaults ADD COLUMN card BLOB;
CREATE TABLE chat_gate_notices_no_sync (
  peer_vault TEXT PRIMARY KEY NOT NULL,
  answer TEXT NOT NULL,
  created_at INTEGER NOT NULL
);";
