ALTER TABLE `haex_space_devices` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_space_members` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_peer_shares` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_mls_sync_keys` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_device_mls_enrollments` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` ADD `haex_column_sigs` TEXT NOT NULL DEFAULT '{}';--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` DROP COLUMN `authored_by_did`;
