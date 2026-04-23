ALTER TABLE `haex_device_mls_enrollments` ADD `authored_by_did` text;--> statement-breakpoint
ALTER TABLE `haex_mls_sync_keys` ADD `authored_by_did` text;--> statement-breakpoint
ALTER TABLE `haex_peer_shares` ADD `authored_by_did` text;--> statement-breakpoint
ALTER TABLE `haex_space_devices` ADD `authored_by_did` text;--> statement-breakpoint
ALTER TABLE `haex_space_members` ADD `authored_by_did` text;