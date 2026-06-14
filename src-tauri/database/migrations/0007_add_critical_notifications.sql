-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Creates haex_critical_notifications_no_sync — local-only persistent record
-- of mutex-poison / DB-schema-drift / audit-log-write-failure conditions.
-- See docs/plans/2026-06-13-critical-failure-pattern.md for the full design.
--
-- Why this is a separate table (not haex_logs):
--   The `CriticalNotificationSink` runs against a SEPARATE SQLite connection
--   so it can still write when the main vault connection's mutex is
--   poisoned. Routing through the haex_logs path would re-enter the very
--   lock that just failed.
--
-- Why `_no_sync`:
--   These are local-machine alerts ("your vault is in a broken state, please
--   restart") — never CRDT-synced to peers. The `_no_sync` suffix is the
--   project convention for tables outside the CRDT scope.
--
-- Why no `haex_hlc` / `haex_column_hlcs` columns:
--   `_no_sync` tables don't run through `execute_with_crdt`. Plain SQL only.
--
-- Dedup invariant (Q3 in the plan):
--   UNIQUE INDEX on (code, location, acknowledged) collapses repeated
--   failures into count++ via UPSERT. When the user acknowledges a row, it
--   keeps existing (with acknowledged=1) and a new occurrence creates a
--   fresh unacked row — the banner reappears on the next failure of the
--   same kind.
-- ---------------------------------------------------------------------------

CREATE TABLE `haex_critical_notifications_no_sync` (
  `id` text PRIMARY KEY NOT NULL,
  `code` text NOT NULL,
  `location` text NOT NULL,
  `params` text NOT NULL,
  `count` integer NOT NULL DEFAULT 1,
  `first_seen` text NOT NULL,
  `last_seen` text NOT NULL,
  `acknowledged` integer NOT NULL DEFAULT 0
);
--> statement-breakpoint
-- Banner queries: WHERE acknowledged = 0 ORDER BY last_seen DESC LIMIT 1
CREATE INDEX `haex_critical_notifications_unacked_idx`
  ON `haex_critical_notifications_no_sync` (`acknowledged`, `last_seen`)
  WHERE `acknowledged` = 0;
--> statement-breakpoint
-- UPSERT dedup key (Q3): same (code, location) on a still-unacked row gets
-- count++ instead of a new INSERT. After acknowledge, the next occurrence
-- creates a NEW (acknowledged = 0) row because the tuple differs.
CREATE UNIQUE INDEX `haex_critical_notifications_dedup_idx`
  ON `haex_critical_notifications_no_sync` (`code`, `location`, `acknowledged`);
