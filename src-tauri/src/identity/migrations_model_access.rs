//! SQL of migration `0037_extension_model_access` (spec 055, data-model.md, research R5 and R9):
//! the rank of the models an extension may use, and the run time of a model job as one more limit.
//!
//! `extension_model_order` is vault-wide and syncs, so the order the user chose holds on every
//! own device. It carries no UNIQUE constraint (a conflict would halt the sync, as in
//! [`super::migrations_storage`]): the id derives from the extension and the model
//! (`extensions::ids::model_order_id`), so two devices that rank the same model write the same
//! row. The column of `extension_limits` raises [`super::migrations::HOLZI_TRIGGER_VERSION`], as
//! does the new table.

/// The migration `0037_extension_model_access`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const MODEL_ACCESS_0037: &str = "CREATE TABLE extension_model_order (\
  id TEXT PRIMARY KEY NOT NULL, \
  extension_id TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE, \
  model_id TEXT NOT NULL, \
  position INTEGER NOT NULL\
);\n\
--> statement-breakpoint\n\
ALTER TABLE extension_limits ADD COLUMN model_job_timeout_ms INTEGER NOT NULL DEFAULT 300000;";

/// The tables of [`MODEL_ACCESS_0037`] that haex-crdt tracks and syncs.
#[cfg(test)]
pub const SYNCED_TABLES: [&str; 1] = ["extension_model_order"];
