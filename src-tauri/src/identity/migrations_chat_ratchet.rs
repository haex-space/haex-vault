//! SQL of migration `0063_chat_ratchet` (spec 095, data-model.md, research R9): the device sessions
//! of the chat with contacts.
//!
//! Every table is `_no_sync`, so haex-crdt neither tracks nor syncs it and
//! [`super::migrations::HOLZI_TRIGGER_VERSION`] stays. The tables with secrets (the ratchet account
//! and the sessions, as pickled JSON) and the state about peer devices are keyed by
//! `installation_uuid` like `device_keys_no_sync`: a vault file copied to another installation
//! carries the rows, but that installation never reads them and makes its own. Chat key records are
//! public and kept per peer device. The outbox keeps its rows and gains what spec 095 sends again:
//! the vaults still awaiting a delivery receipt, when a Nostr-Relay first accepted, how often it was
//! encrypted again, and per receiving device the finished wrap that retries publish unchanged.
//! Device keys and vaults are hex text as in the other chat tables.

/// The migration `0063_chat_ratchet`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const CHAT_RATCHET_0063: &str = r#"CREATE TABLE chat_ratchet_account_no_sync (
  installation_uuid TEXT PRIMARY KEY NOT NULL,
  account TEXT NOT NULL,
  start_key_made_at INTEGER NOT NULL,
  record_published_at INTEGER
);
--> statement-breakpoint
CREATE TABLE chat_ratchet_sessions_no_sync (
  installation_uuid TEXT NOT NULL,
  peer_device TEXT NOT NULL,
  session_id TEXT NOT NULL,
  session TEXT NOT NULL,
  peer_identity_key TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_received_at INTEGER,
  last_sent_at INTEGER,
  own_ratchet_key TEXT,
  own_key_since INTEGER,
  renewed_at INTEGER,
  PRIMARY KEY (installation_uuid, peer_device, session_id)
);
--> statement-breakpoint
CREATE TABLE chat_ratchet_peers_no_sync (
  installation_uuid TEXT NOT NULL,
  peer_device TEXT NOT NULL,
  reset_sent_at INTEGER,
  reset_received_at INTEGER,
  recovering_since INTEGER,
  refetch_record INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (installation_uuid, peer_device)
);
--> statement-breakpoint
CREATE TABLE chat_key_records_no_sync (
  peer_device TEXT PRIMARY KEY NOT NULL,
  identity_key TEXT NOT NULL,
  start_key TEXT NOT NULL,
  made_at INTEGER NOT NULL,
  valid_until INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_outbox_sealed_no_sync (
  outbox_id TEXT NOT NULL,
  recipient_device TEXT NOT NULL,
  wrap TEXT NOT NULL,
  accepted_at INTEGER,
  PRIMARY KEY (outbox_id, recipient_device)
);
--> statement-breakpoint
ALTER TABLE chat_outbox_no_sync ADD COLUMN awaiting TEXT NOT NULL DEFAULT '[]';
--> statement-breakpoint
ALTER TABLE chat_outbox_no_sync ADD COLUMN accepted_at INTEGER;
--> statement-breakpoint
ALTER TABLE chat_outbox_no_sync ADD COLUMN resends INTEGER NOT NULL DEFAULT 0;"#;

/// The tables [`CHAT_RATCHET_0063`] creates; none of them is tracked by haex-crdt.
#[cfg(test)]
pub const DEVICE_TABLES: [&str; 5] = [
    "chat_ratchet_account_no_sync",
    "chat_ratchet_sessions_no_sync",
    "chat_ratchet_peers_no_sync",
    "chat_key_records_no_sync",
    "chat_outbox_sealed_no_sync",
];
