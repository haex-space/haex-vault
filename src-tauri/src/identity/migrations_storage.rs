//! SQL of migration `0028_storage_connections` (spec 038, data-model.md, research R3): the storage
//! connections of holzi (provider, endpoint, region, addressing; the credentials are an entry of the
//! password manager owned by `storage`), the storages on them (one bucket each, the target of the
//! extension permission `remoteStorage`) and the last test result per storage on this device.
//!
//! Connections and storages are vault-wide and sync (spec 038 FR-006); the test result is an
//! observation of one device, so it stays here. Synced tables carry no UNIQUE constraint (a conflict
//! would halt the sync, as in [`super::migrations_extensions`]); the field rules are checked in
//! `remote_storage::store`. New CRDT tables raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0028_storage_connections`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const STORAGE_0028: &str = r#"CREATE TABLE haex_storage_connections (
  id TEXT PRIMARY KEY NOT NULL,
  provider_name TEXT NOT NULL,
  provider_kind TEXT NOT NULL,
  endpoint TEXT NOT NULL,
  endpoint_scope TEXT NOT NULL,
  region TEXT NOT NULL,
  addressing TEXT NOT NULL,
  credentials_item_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_storages (
  id TEXT PRIMARY KEY NOT NULL,
  connection_id TEXT NOT NULL REFERENCES haex_storage_connections(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  bucket TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
--> statement-breakpoint
CREATE INDEX idx_haex_storages_connection ON haex_storages(connection_id);
--> statement-breakpoint
CREATE TABLE storage_tests_no_sync (
  storage_id TEXT PRIMARY KEY NOT NULL,
  tested_at TEXT NOT NULL,
  outcome TEXT NOT NULL
);"#;

/// The migration `0031_storage_protocols` (spec 050, data-model.md, research R13): the protocol of a
/// connection (existing connections are S3) and the confirmed host key of an SFTP server with the
/// time of its confirmation. `haex_storage_connections` is CRDT-tracked, so
/// [`super::migrations::HOLZI_TRIGGER_VERSION`] is raised with it. `haex_storages.bucket` keeps its
/// name and holds the folder of a WebDAV or SFTP storage.
pub const STORAGE_PROTOCOLS_0031: &str = r#"ALTER TABLE haex_storage_connections ADD COLUMN protocol TEXT NOT NULL DEFAULT 's3';
--> statement-breakpoint
ALTER TABLE haex_storage_connections ADD COLUMN host_key TEXT;
--> statement-breakpoint
ALTER TABLE haex_storage_connections ADD COLUMN host_key_confirmed_at TEXT;"#;

/// The tables of [`STORAGE_0028`] that haex-crdt tracks and syncs.
pub const SYNCED_TABLES: [&str; 2] = ["haex_storage_connections", "haex_storages"];

/// The tables of [`STORAGE_0028`] that stay on this device.
pub const DEVICE_TABLES: [&str; 1] = ["storage_tests_no_sync"];
