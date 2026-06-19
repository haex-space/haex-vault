-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Creates haex_principals — the unified actor-identity table for the permission
-- model. A principal is whatever can be granted permissions: today an installed
-- extension or an authorized external client (`kind` = 'extension' |
-- 'external_client').
--
-- This migration is CREATE TABLE only — no data backfill. Seeding existing
-- extensions/clients as principals happens at vault open via
-- execute_with_crdt so each row carries proper HLC timestamps and participates
-- in CRDT sync — direct INSERTs in this migration would bypass the trigger and
-- produce rows with haex_hlc=NULL that future edits can't merge cleanly.
--
-- CRDT columns (haex_hlc, haex_column_hlcs) are injected automatically by
-- the Rust CrdtTransformer — do NOT add them here.
-- ---------------------------------------------------------------------------

CREATE TABLE `haex_principals` (
  `id` text PRIMARY KEY NOT NULL,
  `kind` text NOT NULL,
  `public_key` text NOT NULL,
  `name` text NOT NULL,
  `enabled` integer DEFAULT 1,
  `created_at` text DEFAULT (CURRENT_TIMESTAMP),
  `updated_at` integer
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_principals_public_key_kind_unique` ON `haex_principals` (`public_key`, `kind`);
