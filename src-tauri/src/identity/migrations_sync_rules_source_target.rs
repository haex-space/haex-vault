//! SQL of migration `0053_sync_rules_source_target` (spec 075): the two places of a sync rule are the
//! source and the target, not "A" and "B". It renames the stored columns of `0044_sync_rules` and
//! rewrites the stored direction and side values, so rules, the per-file state, pending deletions and
//! the run log of an existing vault keep their meaning.
//!
//! The tables are `_no_sync` (ADR 0018), so [`super::migrations::HOLZI_TRIGGER_VERSION`] stays. Every
//! `UPDATE` changes only the old values (`CASE … ELSE side END`): a value it does not know stays as it
//! is, and the readers keep ignoring such a row as they do for one a newer version wrote.

/// The migration `0053_sync_rules_source_target`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const SYNC_RULES_SOURCE_TARGET_0053: &str = r#"ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_a_source TO source_location;
--> statement-breakpoint
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_a_path TO source_path;
--> statement-breakpoint
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_b_source TO target_location;
--> statement-breakpoint
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_b_path TO target_path;
--> statement-breakpoint
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN a_size TO source_size;
--> statement-breakpoint
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN a_mtime_ms TO source_mtime_ms;
--> statement-breakpoint
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN b_size TO target_size;
--> statement-breakpoint
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN b_mtime_ms TO target_mtime_ms;
--> statement-breakpoint
UPDATE haex_sync_rules_no_sync SET direction = 'sourceToTarget' WHERE direction = 'aToB';
--> statement-breakpoint
UPDATE haex_sync_rule_pending_no_sync
  SET side = CASE side WHEN 'a' THEN 'source' WHEN 'b' THEN 'target' ELSE side END;
--> statement-breakpoint
UPDATE haex_sync_run_items_no_sync
  SET side = CASE side WHEN 'a' THEN 'source' WHEN 'b' THEN 'target' ELSE side END;"#;
