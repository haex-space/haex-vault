//! SQL of migration `0038_browser_clients` (spec 049, data-model.md, research R13): the remembered
//! approvals and blocks of browser extensions that connect through the External Bridge.
//!
//! A row belongs to one device (ADR-0001): the extension lives in a browser on that device and
//! connects only to holzi there, so `vault_device_uuid` references `known_devices` and the bridge
//! reads only its own device's rows. The table syncs like every device-scoped table; another
//! device never reads the rows. It carries no UNIQUE constraint (a conflict would halt the sync,
//! as in [`super::migrations_storage`]): the id derives from the device and the client
//! (`browser_bridge::approvals::row_id`), so one client has one row per device. New CRDT tables
//! raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0038_browser_clients`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const BROWSER_CLIENTS_0038: &str = r#"CREATE TABLE haex_browser_clients (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  client_id TEXT NOT NULL,
  public_key TEXT NOT NULL,
  client_name TEXT NOT NULL,
  origin TEXT NOT NULL,
  declaration TEXT NOT NULL,
  status TEXT NOT NULL,
  access TEXT,
  scope_tags TEXT,
  default_tag TEXT,
  write_denied INTEGER NOT NULL DEFAULT 0,
  decided_at INTEGER NOT NULL,
  last_seen_at INTEGER
);"#;

/// The tables of [`BROWSER_CLIENTS_0038`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 1] = ["haex_browser_clients"];
