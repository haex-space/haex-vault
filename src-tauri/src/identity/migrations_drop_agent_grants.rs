//! SQL of migration `0056_drop_agent_grants` (spec 083 T072, research R8): the per-agent rights of
//! spec 021 are gone. Since PR B nothing reads `haex_mcp_agent_grants`; the rights of an agent come
//! from its agent profile, and the old rows were never converted (FR-024). The table goes now, so
//! the rights an agent once had do not linger in the vault.
//!
//! `DROP TABLE` fires no triggers, so no delete marker reaches other devices (precedent:
//! `0020_wm_session_no_sync`, `0043_contacts_cards_hash`): every device drops its own copy when it
//! runs this migration. Migration `0045` and the earlier ones that created or changed the table
//! stay as they are. `HOLZI_TRIGGER_VERSION` is raised as for every change of the tracked tables.

/// The migration `0056_drop_agent_grants`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const DROP_AGENT_GRANTS_0056: &str = r#"DROP TABLE haex_mcp_agent_grants;
"#;
