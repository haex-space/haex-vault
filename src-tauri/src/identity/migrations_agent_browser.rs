//! SQL of migration `0047_agent_browser` (spec 065, data-model.md, research R3): the key of the
//! agent browser's profile archive on this device. The table stays on the device (`_no_sync`), so
//! the key never reaches another device and an archive copied there stays unreadable. No CRDT
//! table, so [`super::migrations::HOLZI_TRIGGER_VERSION`] stays.

/// The migration `0047_agent_browser`; registered in [`super::migrations::holzi_migration_source`].
pub const AGENT_BROWSER_0047: &str = r#"CREATE TABLE haex_agent_browser_no_sync (
  vault_device_uuid TEXT PRIMARY KEY NOT NULL,
  profile_key BLOB NOT NULL,
  browser_path TEXT,
  created_at INTEGER NOT NULL
);"#;

/// The table of [`AGENT_BROWSER_0047`].
#[cfg(test)]
pub const TABLE: &str = "haex_agent_browser_no_sync";
