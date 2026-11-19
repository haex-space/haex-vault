//! SQL of migration `0023_extensions` (spec 017-extension-host, data-model.md): the registry of
//! installed haextensions, their bundles, migrations, permissions, limits and per-device state.
//! Tables that an extension creates itself (`<publicKey>__<name>__<table>`) are not here; they come
//! from the extension's own migrations at run time (research R8).
//!
//! Synced tables carry no UNIQUE constraint: a UNIQUE conflict halts the sync of haex-crdt, so
//! uniqueness comes from derived UUIDv5 ids (`extensions::ids`, research R5). haex-crdt adds the
//! HLC columns and triggers to every table without the `_no_sync` suffix, so none of them is in the
//! SQL. Rows that describe this device follow ADR-0001 (`vault_device_uuid` with a foreign key to
//! `known_devices`; the nil UUID means vault-wide). The `_no_sync` tables describe the state of this
//! vault file (migration journal, applied purges, logs, parked sync groups, developer mode) and
//! never sync. `identity/migrations.rs` is over the 500-line boundary already, so the SQL lives here
//! and that file only registers it.

/// The migration `0023_extensions`; registered in [`super::migrations::haex_migration_source`].
pub const EXTENSIONS_0023: &str = r#"CREATE TABLE extensions (
  id TEXT PRIMARY KEY NOT NULL,
  public_key TEXT NOT NULL,
  name TEXT NOT NULL,
  display_name TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL DEFAULT 'installed',
  purge_data INTEGER NOT NULL DEFAULT 0,
  purge_hlc TEXT,
  installed_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_bundles (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  version TEXT NOT NULL,
  manifest_json BLOB NOT NULL,
  signature_json BLOB NOT NULL,
  retired INTEGER NOT NULL DEFAULT 0,
  added_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_bundle_files (
  id TEXT PRIMARY KEY NOT NULL,
  bundle_id TEXT NOT NULL REFERENCES extension_bundles(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  size INTEGER NOT NULL,
  sha256 TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_blobs (
  hash TEXT PRIMARY KEY NOT NULL,
  data BLOB NOT NULL,
  size INTEGER NOT NULL,
  orphaned_at INTEGER
);
--> statement-breakpoint
CREATE TABLE extension_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  position INTEGER NOT NULL,
  sql TEXT NOT NULL,
  sql_sha256 TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_permissions (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  status TEXT NOT NULL,
  declared INTEGER NOT NULL DEFAULT 0,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_limits (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  max_rows INTEGER NOT NULL DEFAULT 10000,
  max_concurrent INTEGER NOT NULL DEFAULT 20,
  max_sql_bytes INTEGER NOT NULL DEFAULT 1000000,
  timeout_ms INTEGER NOT NULL DEFAULT 5000,
  max_response_bytes INTEGER NOT NULL DEFAULT 16777216
);
--> statement-breakpoint
CREATE TABLE extension_device_status (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  status TEXT NOT NULL,
  bundle_id TEXT,
  error TEXT,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_kv (
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (vault_device_uuid, extension_id, key)
);
--> statement-breakpoint
CREATE TABLE extension_migrations_applied_no_sync (
  extension_id TEXT NOT NULL,
  name TEXT NOT NULL,
  sql_sha256 TEXT NOT NULL,
  applied_at INTEGER NOT NULL,
  PRIMARY KEY (extension_id, name)
);
--> statement-breakpoint
CREATE TABLE extension_purges_applied_no_sync (
  extension_id TEXT PRIMARY KEY NOT NULL,
  purge_hlc TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE extension_logs_no_sync (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  extension_id TEXT NOT NULL,
  vault_device_uuid TEXT NOT NULL,
  level TEXT NOT NULL,
  message TEXT NOT NULL,
  metadata TEXT,
  created_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE sync_parked_groups_no_sync (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  origin TEXT NOT NULL,
  hlc TEXT NOT NULL,
  extension_prefix TEXT NOT NULL,
  tables TEXT NOT NULL,
  group_blob BLOB NOT NULL,
  bytes INTEGER NOT NULL,
  reason TEXT NOT NULL,
  parked_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE dev_extensions_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  public_key TEXT NOT NULL,
  name TEXT NOT NULL,
  display_name TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  vault_device_uuid TEXT NOT NULL,
  project_path TEXT NOT NULL,
  dev_url TEXT NOT NULL,
  installed_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE TABLE dev_extension_permissions_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  extension_id TEXT NOT NULL REFERENCES dev_extensions_no_sync(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  status TEXT NOT NULL,
  declared INTEGER NOT NULL DEFAULT 0,
  vault_device_uuid TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
--> statement-breakpoint
CREATE INDEX idx_ext_bundles_ext ON extension_bundles(extension_id);
--> statement-breakpoint
CREATE INDEX idx_ext_bfiles_bundle ON extension_bundle_files(bundle_id);
--> statement-breakpoint
CREATE INDEX idx_ext_bfiles_hash ON extension_bundle_files(sha256);
--> statement-breakpoint
CREATE INDEX idx_ext_mig_ext ON extension_migrations(extension_id, position);
--> statement-breakpoint
CREATE INDEX idx_ext_perm_ext ON extension_permissions(extension_id, kind);
--> statement-breakpoint
CREATE INDEX idx_ext_devst_ext ON extension_device_status(extension_id);
--> statement-breakpoint
CREATE INDEX idx_ext_logs_ext ON extension_logs_no_sync(extension_id, created_at);
--> statement-breakpoint
CREATE INDEX idx_sync_parked_prefix ON sync_parked_groups_no_sync(extension_prefix, hlc);"#;

/// The migration `0024_dev_extension_kv` (spec 017, US12): the key-value store of an extension
/// loaded in developer mode. Like its registration it stays on this device; unloading it deletes
/// the rows.
pub const DEV_EXTENSION_KV_0024: &str = r#"CREATE TABLE dev_extension_kv_no_sync (
  extension_id TEXT NOT NULL REFERENCES dev_extensions_no_sync(id) ON DELETE CASCADE,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (extension_id, key)
);"#;

/// The tables of [`EXTENSIONS_0023`] that haex-crdt tracks and syncs.
pub const SYNCED_TABLES: [&str; 9] = [
    "extensions",
    "extension_bundles",
    "extension_bundle_files",
    "extension_blobs",
    "extension_migrations",
    "extension_permissions",
    "extension_limits",
    "extension_device_status",
    "extension_kv",
];

/// The tables of [`EXTENSIONS_0023`] that stay on this device.
pub const DEVICE_TABLES: [&str; 6] = [
    "extension_migrations_applied_no_sync",
    "extension_purges_applied_no_sync",
    "extension_logs_no_sync",
    "sync_parked_groups_no_sync",
    "dev_extensions_no_sync",
    "dev_extension_permissions_no_sync",
];

/// The migration `0025_sync_parking` (spec 017, research R10, R11): parked groups are unique per
/// origin and HLC (a group fetched again is not stored or counted twice, and the check is an index
/// lookup), and a device remembers the highest "delete data" removal it cleared up for, so a later
/// "keep data" removal does not lift the filter for changes older than it.
pub const SYNC_PARKING_0025: &str = r#"DELETE FROM sync_parked_groups_no_sync
  WHERE id NOT IN (SELECT MIN(id) FROM sync_parked_groups_no_sync GROUP BY origin, hlc);
--> statement-breakpoint
CREATE UNIQUE INDEX idx_sync_parked_origin_hlc ON sync_parked_groups_no_sync(origin, hlc);
--> statement-breakpoint
ALTER TABLE extension_purges_applied_no_sync ADD COLUMN data_purge_hlc TEXT;"#;
