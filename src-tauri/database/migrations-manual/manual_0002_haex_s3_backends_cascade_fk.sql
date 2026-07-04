-- ---------------------------------------------------------------------------
-- MANUAL MIGRATION (hand-written; drizzle-kit cannot emit CASCADE via ALTER)
-- ---------------------------------------------------------------------------
-- The drizzle schema declares
--     parent_backend_id: text('parent_backend_id')
--         .references((): AnySQLiteColumn => haexS3Backends.id, { onDelete: 'cascade' })
-- but 0006_rename_storage_backends_to_s3_backends.sql added the column via
-- `ALTER TABLE ADD COLUMN`, and SQLite cannot express `ON DELETE CASCADE` on
-- an added FK column that way — the FK is present but the cascade is not.
--
-- This migration rebuilds haex_s3_backends via the SQLite table-rebuild
-- pattern so the self-referential FK on parent_backend_id gets the
-- enforced CASCADE it was always supposed to have. Data-preserving.
--
-- Notes:
--   - CRDT columns (haex_hlc, haex_column_hlcs) are injected at runtime by
--     the Rust CrdtTransformer's `transform_ddl_statement` path (the
--     __new_haex_s3_backends table name does NOT end in `_no_sync`, so it is
--     treated as a syncable table). Do NOT list them here — the transformer
--     adds them to the CREATE TABLE column set, and the INSERT SELECT
--     omits them (matches the drizzle 0001_plain_onslaught rebuild pattern
--     for syncable tables).
--   - PRAGMA foreign_keys OFF/ON pair mirrors the drizzle rebuild pattern.
-- ---------------------------------------------------------------------------

PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_s3_backends` (
    `id` text PRIMARY KEY NOT NULL,
    `type` text NOT NULL,
    `name` text NOT NULL,
    `config` text NOT NULL,
    `enabled` integer DEFAULT true NOT NULL,
    `parent_backend_id` text REFERENCES `haex_s3_backends`(`id`) ON DELETE CASCADE,
    `origin_type` text DEFAULT 'owned' NOT NULL,
    `share_prefix` text,
    `share_access_flags` integer,
    `created_at` text DEFAULT (CURRENT_TIMESTAMP)
);--> statement-breakpoint
INSERT INTO `__new_haex_s3_backends`("id", "type", "name", "config", "enabled", "parent_backend_id", "origin_type", "share_prefix", "share_access_flags", "created_at") SELECT "id", "type", "name", "config", "enabled", "parent_backend_id", "origin_type", "share_prefix", "share_access_flags", "created_at" FROM `haex_s3_backends`;--> statement-breakpoint
DROP TABLE `haex_s3_backends`;--> statement-breakpoint
ALTER TABLE `__new_haex_s3_backends` RENAME TO `haex_s3_backends`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_s3_backends_name_unique` ON `haex_s3_backends` (`name`);
