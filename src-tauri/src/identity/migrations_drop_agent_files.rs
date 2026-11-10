//! SQL of migration `0059_drop_agent_files` (spec 083 FR-033): what an agent may do with files is a
//! right of its agent profile now, so `agent_file_permissions` (spec 044, 048, 056, 061, and the
//! contacts rows of spec 051) is not read any more and goes, like `haex_mcp_agent_grants` did.
//! The old rows were not converted (no users; FR-024 for the other rights).
//!
//! `DROP TABLE` fires no triggers, so no delete marker reaches other devices: every device drops
//! its own copy when it runs this migration. The migrations that created or changed the table stay
//! as they are. `HOLZI_TRIGGER_VERSION` is raised as for every change of the tracked tables.

/// The migration `0059_drop_agent_files`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const DROP_AGENT_FILES_0059: &str = r#"DROP TABLE agent_file_permissions;
"#;
