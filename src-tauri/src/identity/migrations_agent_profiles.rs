//! SQL of migration `0054_agent_profiles` (spec 083, data-model.md): the agent profiles, the rights
//! rows of a profile and the assignment of an agent to a profile.
//!
//! Unlike the agent tables of `0045_mcp_agents`, which belong to one device each (ADR-0001), these
//! three CRDT tables belong to the vault: a profile is the same on every device, so none of them
//! carries `vault_device_uuid`. None has a UNIQUE constraint besides its primary key (a conflict
//! would halt the sync) or a cascading foreign key (deleting a profile deletes its rows
//! explicitly in the same write, so each leaves its own delete marker). "At most one profile per
//! agent" holds by the primary key of `haex_agent_profile_members`, whose id derives from the
//! subject alone, so two devices that assign the same subject write the same row. The rows of
//! `haex_agent_profile_grants` have the shape of `haex_mcp_agent_grants` (`grant_group`,
//! `grant_key`, `level`, `scope_tags`; the column names avoid the SQL keywords `group` and `key`).
//! New CRDT tables raise [`super::migrations::HOLZI_TRIGGER_VERSION`].

/// The migration `0054_agent_profiles`; registered in [`super::migrations::holzi_migration_source`].
pub const AGENT_PROFILES_0054: &str = r#"CREATE TABLE haex_agent_profiles (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL
);
--> statement-breakpoint
CREATE TABLE haex_agent_profile_grants (
  id TEXT PRIMARY KEY NOT NULL,
  profile_id TEXT NOT NULL,
  grant_group TEXT NOT NULL,
  grant_key TEXT NOT NULL DEFAULT '',
  level TEXT NOT NULL,
  scope_tags TEXT
);
--> statement-breakpoint
CREATE TABLE haex_agent_profile_members (
  id TEXT PRIMARY KEY NOT NULL,
  subject TEXT NOT NULL,
  profile_id TEXT NOT NULL
);"#;

/// Non-unique lookup indexes for profile membership and rights resolution.
pub const AGENT_PROFILE_INDEXES_0055: &str = r#"CREATE INDEX haex_agent_profile_grants_profile_id
  ON haex_agent_profile_grants(profile_id);
--> statement-breakpoint
CREATE INDEX haex_agent_profile_members_profile_id
  ON haex_agent_profile_members(profile_id);"#;

/// The tables of [`AGENT_PROFILES_0054`]; all three are tracked and synced by haex-crdt.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 3] = [
    "haex_agent_profiles",
    "haex_agent_profile_grants",
    "haex_agent_profile_members",
];
