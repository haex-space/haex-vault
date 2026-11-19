//! SQL of migration `0064_sync_runs_timing`: a run of a sync rule notes where its time went (listing,
//! reading files for their hash, copying), how many bytes it copied and over which route it reached
//! another device (direct or by a relay, with the round trip time). Runs from before it show zeros and
//! no route.
//!
//! The table is `_no_sync` (ADR 0018), so [`super::migrations::HAEX_TRIGGER_VERSION`] stays.

/// The migration `0064_sync_runs_timing`; registered in [`super::migrations::haex_migration_source`].
pub const SYNC_RUNS_TIMING_0064: &str = r#"ALTER TABLE haex_sync_runs_no_sync ADD COLUMN scan_ms INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE haex_sync_runs_no_sync ADD COLUMN hash_ms INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE haex_sync_runs_no_sync ADD COLUMN copy_ms INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE haex_sync_runs_no_sync ADD COLUMN bytes INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE haex_sync_runs_no_sync ADD COLUMN route TEXT;
--> statement-breakpoint
ALTER TABLE haex_sync_runs_no_sync ADD COLUMN rtt_ms INTEGER;"#;
