//! SQL of migration `0057_agent_memberships` (spec 083 FR-006, data-model.md "Migration 0057"): a
//! subject can be a member of several agent profiles, so the member rows of
//! `haex_agent_profile_members` are looked up by subject and no longer by an id that derives from the
//! subject alone. The migration adds the lookup index and nothing else: the rows that exist keep their
//! ids and stay memberships (`agent_profiles::legacy_member_id`), because rewriting them would insert
//! new rows that a later sync could take for newer than a removal made on another device.
//!
//! The index is not unique (a conflict would halt the sync). A device that still reads one member
//! row per subject would give a subject in a restrictive profile the rights of "Standard", so
//! [`super::migrations::HOLZI_TRIGGER_VERSION`] is raised: the sync handshake refuses it until it is
//! updated.

/// The migration `0057_agent_memberships`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const AGENT_MEMBERSHIPS_0057: &str = r#"CREATE INDEX haex_agent_profile_members_subject
  ON haex_agent_profile_members(subject);"#;
