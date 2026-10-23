//! SQL of migration `0039_file_shares` (spec 056, data-model.md): file shares a device serves to
//! contacts, their grants per contact vault, the shares this vault received, and the switch that
//! lets other own devices reach this device's files.
//!
//! A share belongs to the device that serves it (FR-018), so `vault_device_uuid` references
//! `known_devices`; it still syncs to every own device, which may change or revoke it (FR-019). No
//! table carries a UNIQUE constraint (a conflict would halt the sync, as in
//! [`super::migrations_storage`]): ids derive from what makes a row unique
//! (`files::share::store::share_id`, `grant_id`, `received_id`). The switch lives in a `_no_sync`
//! table so no other device can turn it back on (research R12). New CRDT tables raise
//! [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0039_file_shares`; registered in [`super::migrations::holzi_migration_source`].
pub const FILE_SHARES_0039: &str = r#"CREATE TABLE haex_file_shares (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  source TEXT NOT NULL,
  path TEXT NOT NULL,
  item_kind TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  changed_at TEXT NOT NULL,
  served_at TEXT,
  source_state TEXT NOT NULL DEFAULT 'ok'
);
--> statement-breakpoint
CREATE TABLE haex_file_share_grants (
  id TEXT PRIMARY KEY NOT NULL,
  share_id TEXT NOT NULL REFERENCES haex_file_shares(id) ON DELETE CASCADE,
  vault_pubkey TEXT NOT NULL,
  level TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE INDEX haex_file_share_grants_share ON haex_file_share_grants(share_id);
--> statement-breakpoint
CREATE TABLE haex_file_shares_received (
  id TEXT PRIMARY KEY NOT NULL,
  owner_vault_pubkey TEXT NOT NULL,
  share_id TEXT NOT NULL,
  name TEXT NOT NULL,
  item_kind TEXT NOT NULL,
  level TEXT NOT NULL,
  owner_device_pubkey TEXT NOT NULL,
  owner_endpoint_id TEXT NOT NULL,
  last_reached_at TEXT
);
--> statement-breakpoint
CREATE TABLE haex_file_device_access_no_sync (
  id INTEGER PRIMARY KEY NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);"#;

/// The tables of [`FILE_SHARES_0039`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 3] = [
    "haex_file_shares",
    "haex_file_share_grants",
    "haex_file_shares_received",
];
