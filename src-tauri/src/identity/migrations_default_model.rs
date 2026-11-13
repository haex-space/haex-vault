//! SQL of migration `0062_extension_default_model` (spec 055, change of 2026-11-13): an extension
//! has no rank of models any more, only at most one default model, which its jobs go to first and
//! which holzi asks the user to replace when it cannot take a job.
//!
//! `extension_default_model` is vault-wide and syncs, so the default holds on every own device. It
//! carries no UNIQUE constraint (a conflict would halt the sync, as in [`super::migrations_storage`]):
//! its `id` is the extension's id, so there is at most one row per extension and two devices that
//! set the default write the same row. `extension_model_order` goes: `DROP TABLE` fires no
//! triggers, so no delete marker reaches other devices (precedent: `0056_drop_agent_grants`) and
//! every device drops its own copy. The ranks the users set are not converted. A new tracked table
//! and a dropped one raise `HOLZI_TRIGGER_VERSION`.

/// The migration `0062_extension_default_model`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const DEFAULT_MODEL_0062: &str = "DROP TABLE extension_model_order;\n\
--> statement-breakpoint\n\
CREATE TABLE extension_default_model (\
  id TEXT PRIMARY KEY NOT NULL, \
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE, \
  model_id TEXT NOT NULL\
);";

/// The tables of [`DEFAULT_MODEL_0062`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 1] = ["extension_default_model"];
