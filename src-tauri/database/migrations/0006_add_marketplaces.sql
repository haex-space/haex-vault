-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Creates haex_marketplaces and seeds the built-in haex.space default row.
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

INSERT INTO `haex_marketplaces` (`id`, `name`, `base_url`, `enabled`, `is_default`, `sort_order`, `auth_type`)
VALUES ('00000000-0000-0000-0000-000000000001', 'Haex Marketplace', 'https://marketplace.haex.space', 1, 1, 1, 'none');
