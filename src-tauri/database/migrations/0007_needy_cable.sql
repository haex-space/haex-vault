CREATE TABLE `haex_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`platform` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_devices_endpoint_id_unique` ON `haex_devices` (`endpoint_id`);--> statement-breakpoint
ALTER TABLE `haex_sync_rules` ADD `device_id` text NOT NULL REFERENCES haex_devices(id);