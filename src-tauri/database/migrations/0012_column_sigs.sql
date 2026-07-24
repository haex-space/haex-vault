-- Phase 1: per-column author signatures (`haex_column_sigs`).
--
-- - Adds `haex_column_sigs TEXT NOT NULL DEFAULT '{}'` to every space-scoped
--   CRDT table (parallel to `haex_column_hlcs`), storing a JSON map of
--   {column: signature} authored by the writing device.
-- - Drops `authored_by_did` — row-level authorship is meaningless once every
--   column is co-authored via `haex_column_sigs`.
-- - Drops the `*_ensure_refs` BEFORE INSERT triggers that used to auto-create
--   FK-parent stubs (haex_identities + haex_devices) based on
--   `authored_by_did`. Stub creation moves into Rust (see ADR 0002, Task G1).
-- - `haex_space_devices_propagate_meta` is intentionally kept: it does not
--   depend on `authored_by_did` and still mirrors updates onto foreign device
--   stubs (`secret_key IS NULL`).
DROP TRIGGER IF EXISTS `haex_space_devices_ensure_refs`;--> statement-breakpoint
DROP TRIGGER IF EXISTS `haex_peer_shares_ensure_refs`;--> statement-breakpoint
ALTER TABLE `haex_space_devices` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_space_devices` DROP COLUMN `authored_by_did`;--> statement-breakpoint
ALTER TABLE `haex_space_members` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `authored_by_did`;--> statement-breakpoint
ALTER TABLE `haex_peer_shares` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_peer_shares` DROP COLUMN `authored_by_did`;--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` DROP COLUMN `authored_by_did`;--> statement-breakpoint
ALTER TABLE `haex_mls_sync_keys` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_mls_sync_keys` DROP COLUMN `authored_by_did`;--> statement-breakpoint
ALTER TABLE `haex_device_mls_enrollments` ADD `haex_column_sigs` text DEFAULT '{}' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_device_mls_enrollments` DROP COLUMN `authored_by_did`;
