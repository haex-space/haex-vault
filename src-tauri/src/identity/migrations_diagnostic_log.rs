//! SQL of migration `0068_diagnostic_log` (spec 098, research R4): the level of the diagnostic log
//! of this vault file ("Standard" or "Ausführlich"), the time "Ausführlich" ends and the run of the
//! app that switched it on. One row with `id = 1`; no row means "Standard".
//!
//! A `_no_sync` table: the level is a setting of one device and is switched exactly when something
//! is being diagnosed, possibly the sync itself, so it must neither replicate nor add sync changes.
//! [`super::migrations::HAEX_TRIGGER_VERSION`] stays.

/// The migration `0068_diagnostic_log`; registered in [`super::migrations::haex_migration_source`].
pub const DIAGNOSTIC_LOG_0068: &str = "\
CREATE TABLE haex_diagnostic_log_no_sync (\
    id INTEGER PRIMARY KEY NOT NULL, \
    level TEXT NOT NULL, \
    until_ms INTEGER, \
    run_id TEXT\
);";
