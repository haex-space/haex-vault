//! SQL of migration `0047_agent_browser` (spec 065, data-model.md): the agent browser this device
//! uses. The table stays on the device (`_no_sync`), since a browser path means nothing elsewhere.
//! The key of the profile archive is not here but in the system's credential store (research R3),
//! so a copied vault cannot open the archive. No CRDT table, so
//! [`super::migrations::HOLZI_TRIGGER_VERSION`] stays.

/// The migration `0047_agent_browser`; registered in [`super::migrations::holzi_migration_source`].
pub const AGENT_BROWSER_0047: &str = r#"CREATE TABLE haex_agent_browser_no_sync (
  vault_device_uuid TEXT PRIMARY KEY NOT NULL,
  browser_path TEXT,
  created_at INTEGER NOT NULL
);"#;

/// The table of [`AGENT_BROWSER_0047`].
#[cfg(test)]
pub const TABLE: &str = "haex_agent_browser_no_sync";
