//! SQL of migration `0049_chat_contacts` (spec 072, data-model.md, research R4, R6, R9, R13–R15):
//! conversations with contacts and groups next to the agent conversations.
//!
//! `chat_threads` stays the one list of conversations and gains a kind (`agent` for every existing
//! row), a state and the columns of direct and group conversations. Messages of contacts live in
//! their own table so the agent's message roles stay as they are (research R15).
//!
//! No synced table has a UNIQUE constraint (a conflict would halt the sync, as in
//! [`super::migrations_contacts`]); uniqueness comes from ids derived in `chat::peer::ids`. Delivery
//! states are set-once timestamp columns so concurrent writers on own devices merge per column
//! (research R6). The `_no_sync` tables hold what only this device does: its outbox, the wraps it
//! has seen, its cursors at the Nostr-Relays, its uploads to a Blossom-Server, the files it keeps and
//! incomplete group invitations. New CRDT tables and columns raise
//! [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0049_chat_contacts`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const CHAT_CONTACTS_0049: &str = r#"ALTER TABLE chat_threads ADD COLUMN kind TEXT NOT NULL DEFAULT 'agent';
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN state TEXT NOT NULL DEFAULT 'active';
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN peer_vault TEXT;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN muted INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN last_read_at INTEGER;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN picture BLOB;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN meta_at INTEGER;
--> statement-breakpoint
ALTER TABLE chat_threads ADD COLUMN meta_op TEXT;
--> statement-breakpoint
CREATE TABLE chat_members (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL,
  vault TEXT NOT NULL,
  card BLOB,
  state TEXT NOT NULL,
  state_at INTEGER NOT NULL,
  state_op TEXT NOT NULL,
  state_by TEXT NOT NULL,
  joined_at INTEGER
);
--> statement-breakpoint
CREATE INDEX idx_chat_members_thread ON chat_members (thread_id);
--> statement-breakpoint
CREATE TABLE chat_peer_messages (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  idempotency_key TEXT,
  sender_vault TEXT NOT NULL,
  sender_device TEXT NOT NULL,
  kind TEXT NOT NULL,
  text TEXT,
  note TEXT,
  sent_at INTEGER NOT NULL,
  received_at INTEGER NOT NULL,
  sort_at INTEGER NOT NULL,
  inserted_by TEXT NOT NULL
);
--> statement-breakpoint
CREATE INDEX idx_chat_peer_messages_thread ON chat_peer_messages (thread_id, sort_at, id);
--> statement-breakpoint
CREATE TABLE chat_attachments (
  id TEXT PRIMARY KEY NOT NULL,
  message_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  name TEXT,
  mime TEXT,
  size INTEGER,
  sha256 TEXT,
  blossom_url TEXT,
  blob_sha256 TEXT,
  blob_key BLOB,
  contact TEXT,
  photo BLOB
);
--> statement-breakpoint
CREATE INDEX idx_chat_attachments_message ON chat_attachments (message_id);
--> statement-breakpoint
CREATE TABLE chat_deliveries (
  id TEXT PRIMARY KEY NOT NULL,
  message_id TEXT NOT NULL,
  recipient_vault TEXT NOT NULL,
  sent_at INTEGER,
  delivered_at INTEGER,
  read_at INTEGER,
  files_at INTEGER,
  failed_at INTEGER,
  failed_reason TEXT
);
--> statement-breakpoint
CREATE INDEX idx_chat_deliveries_message ON chat_deliveries (message_id);
--> statement-breakpoint
CREATE TABLE chat_outbox_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL,
  message_id TEXT,
  rumor TEXT NOT NULL,
  pending TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_at INTEGER NOT NULL,
  last_error TEXT,
  created_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_wraps_seen_no_sync (
  wrap_id TEXT PRIMARY KEY NOT NULL,
  seen_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_relay_cursors_no_sync (
  relay TEXT PRIMARY KEY NOT NULL,
  newest_created_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_blobs_no_sync (
  blob_sha256 TEXT PRIMARY KEY NOT NULL,
  server TEXT NOT NULL,
  signer TEXT NOT NULL,
  upload_secret BLOB,
  key_entry_id TEXT,
  attachment_id TEXT NOT NULL,
  pending_devices TEXT NOT NULL,
  uploaded_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_attachment_files_no_sync (
  attachment_id TEXT PRIMARY KEY NOT NULL,
  state TEXT NOT NULL,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE chat_group_parts_no_sync (
  group_id TEXT NOT NULL,
  part INTEGER NOT NULL,
  part_count INTEGER NOT NULL,
  payload TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  PRIMARY KEY (group_id, part)
);"#;

/// The tables of [`CHAT_CONTACTS_0049`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 4] = [
    "chat_members",
    "chat_peer_messages",
    "chat_attachments",
    "chat_deliveries",
];

/// The device-local tables of [`CHAT_CONTACTS_0049`].
#[cfg(test)]
pub const DEVICE_TABLES: [&str; 6] = [
    "chat_outbox_no_sync",
    "chat_wraps_seen_no_sync",
    "chat_relay_cursors_no_sync",
    "chat_blobs_no_sync",
    "chat_attachment_files_no_sync",
    "chat_group_parts_no_sync",
];

/// The columns 0049 adds to the existing `chat_threads`.
#[cfg(test)]
pub const THREAD_COLUMNS: [&str; 8] = [
    "kind",
    "state",
    "peer_vault",
    "muted",
    "last_read_at",
    "picture",
    "meta_at",
    "meta_op",
];
