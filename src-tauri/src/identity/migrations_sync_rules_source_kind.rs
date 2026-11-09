//! SQL of migration `0060_sync_rules_source_kind` (spec 074): a sync rule remembers whether its source place
//! is a folder or one file. Every rule until now is a folder rule, so the column is added with that default
//! and no row is touched.
//!
//! The table is `_no_sync` (ADR 0018), so [`super::migrations::HOLZI_TRIGGER_VERSION`] stays.

/// The migration `0060_sync_rules_source_kind`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const SYNC_RULES_SOURCE_KIND_0060: &str =
    r#"ALTER TABLE haex_sync_rules_no_sync ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'folder';"#;
