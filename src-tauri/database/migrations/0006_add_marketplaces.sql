-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Creates haex_marketplaces. The built-in default row is seeded at vault open
-- time via ensureDefaultMarketplaceAsync (drizzle/execute_with_crdt) so it
-- carries proper HLC timestamps and participates in CRDT sync — direct INSERTs
-- in this migration would bypass the trigger and produce a row with
-- haex_hlc=NULL that future user edits can't merge cleanly.
--
-- CRDT columns (haex_hlc, haex_column_hlcs) are injected automatically by
-- the Rust CrdtTransformer — do NOT add them here.
-- ---------------------------------------------------------------------------

CREATE TABLE `haex_marketplaces` (
  `id` text PRIMARY KEY NOT NULL,
  `name` text NOT NULL,
  `base_url` text NOT NULL,
  `enabled` integer NOT NULL DEFAULT 1,
  `is_default` integer NOT NULL DEFAULT 0,
  `sort_order` integer NOT NULL DEFAULT 100,
  `auth_type` text NOT NULL DEFAULT 'none',
  `auth_token` text,
  `auth_username` text,
  `auth_password` text,
  `auth_identity_id` text,
  `created_at` text DEFAULT (CURRENT_TIMESTAMP),
  `updated_at` text DEFAULT (CURRENT_TIMESTAMP)
);
