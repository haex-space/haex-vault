//! SQL of migration `0045_mcp_agents` (spec 021, data-model.md): the external agents registered on
//! this device, their rights, and the log of their calls.
//!
//! The two CRDT tables belong to one device each (ADR-0001), like `haex_browser_clients`: an agent
//! runs on one device and reaches holzi only there, so `vault_device_uuid` references
//! `known_devices` and the server reads only its own device's rows. Neither carries a UNIQUE
//! constraint (a conflict would halt the sync); the ids derive from the device and the agent. The
//! log stays on the device (`_no_sync`) and is cut by age and count when it is written and read.
//! The column names `grant_group` and `grant_key` avoid the SQL keywords `group` and `key`. New
//! CRDT tables raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0045_mcp_agents`; registered in [`super::migrations::holzi_migration_source`].
pub const MCP_AGENTS_0045: &str = r#"CREATE TABLE haex_mcp_agents (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  slug TEXT NOT NULL,
  client_name TEXT NOT NULL,
  client_version TEXT,
  transport TEXT NOT NULL,
  oauth_client_id TEXT,
  token_hash TEXT,
  status TEXT NOT NULL,
  permission_mode TEXT NOT NULL DEFAULT 'manual',
  registered_at INTEGER NOT NULL,
  last_seen_at INTEGER
);
--> statement-breakpoint
CREATE TABLE haex_mcp_agent_grants (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  agent_id TEXT NOT NULL,
  grant_group TEXT NOT NULL,
  grant_key TEXT NOT NULL DEFAULT '',
  level TEXT NOT NULL,
  scope_tags TEXT
);
--> statement-breakpoint
CREATE TABLE haex_mcp_agent_log_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  tool TEXT NOT NULL,
  effect TEXT NOT NULL,
  target TEXT,
  input TEXT,
  outcome TEXT NOT NULL,
  asked INTEGER NOT NULL DEFAULT 0
);
--> statement-breakpoint
CREATE INDEX idx_mcp_agent_log ON haex_mcp_agent_log_no_sync(agent_id, created_at);"#;

/// The tables of [`MCP_AGENTS_0045`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 2] = ["haex_mcp_agents", "haex_mcp_agent_grants"];

/// The device-only table of [`MCP_AGENTS_0045`].
#[cfg(test)]
pub const LOG_TABLE: &str = "haex_mcp_agent_log_no_sync";
