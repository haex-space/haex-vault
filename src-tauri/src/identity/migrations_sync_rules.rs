//! SQL of migration `0044_sync_rules` (spec 061, data-model.md): sync rules, the per-file state of
//! their last reconciliation, deletions waiting for a confirmation, and the run log.
//!
//! Everything stays on the device that runs the rule (FR-003, FR-045; ADR 0018): the tables are
//! `_no_sync`, so haex-crdt neither tracks nor syncs them and
//! [`super::migrations::HOLZI_TRIGGER_VERSION`] stays. Every row still carries
//! `vault_device_uuid`, so a vault file copied to another device runs none of these rules there
//! (ADR 0001).

/// The migration `0044_sync_rules`; registered in [`super::migrations::holzi_migration_source`].
pub const SYNC_RULES_0044: &str = r#"CREATE TABLE haex_sync_rules_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  place_a_source TEXT NOT NULL,
  place_a_path TEXT NOT NULL,
  place_b_source TEXT NOT NULL,
  place_b_path TEXT NOT NULL,
  direction TEXT NOT NULL,
  delete_mode TEXT NOT NULL,
  interval_secs INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_run_at TEXT,
  last_outcome TEXT,
  last_error TEXT
);
--> statement-breakpoint
CREATE TABLE haex_sync_rule_files_no_sync (
  rule_id TEXT NOT NULL REFERENCES haex_sync_rules_no_sync(id) ON DELETE CASCADE,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  path TEXT NOT NULL,
  a_size INTEGER,
  a_mtime_ms INTEGER,
  b_size INTEGER,
  b_mtime_ms INTEGER,
  kept INTEGER NOT NULL DEFAULT 0,
  hash TEXT,
  PRIMARY KEY (rule_id, path)
);
--> statement-breakpoint
CREATE TABLE haex_sync_rule_pending_no_sync (
  rule_id TEXT NOT NULL REFERENCES haex_sync_rules_no_sync(id) ON DELETE CASCADE,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  path TEXT NOT NULL,
  side TEXT NOT NULL,
  found_run_id TEXT NOT NULL,
  PRIMARY KEY (rule_id, path)
);
--> statement-breakpoint
CREATE TABLE haex_sync_runs_no_sync (
  id TEXT PRIMARY KEY NOT NULL,
  rule_id TEXT NOT NULL REFERENCES haex_sync_rules_no_sync(id) ON DELETE CASCADE,
  vault_device_uuid TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  started_ms INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  cause TEXT NOT NULL,
  outcome TEXT NOT NULL,
  error TEXT,
  copied INTEGER NOT NULL,
  moved INTEGER NOT NULL,
  deleted INTEGER NOT NULL,
  conflicts INTEGER NOT NULL,
  errors INTEGER NOT NULL,
  skipped INTEGER NOT NULL
);
--> statement-breakpoint
CREATE INDEX haex_sync_runs_no_sync_rule ON haex_sync_runs_no_sync (rule_id, started_ms);
--> statement-breakpoint
CREATE TABLE haex_sync_run_items_no_sync (
  run_id TEXT NOT NULL REFERENCES haex_sync_runs_no_sync(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  path TEXT NOT NULL,
  action TEXT NOT NULL,
  side TEXT NOT NULL,
  detail TEXT,
  trash_kind TEXT,
  trash_path TEXT,
  PRIMARY KEY (run_id, seq)
);"#;

/// The tables of [`SYNC_RULES_0044`], none of them synced.
#[cfg(test)]
pub const TABLES: [&str; 5] = [
    "haex_sync_rules_no_sync",
    "haex_sync_rule_files_no_sync",
    "haex_sync_rule_pending_no_sync",
    "haex_sync_runs_no_sync",
    "haex_sync_run_items_no_sync",
];
