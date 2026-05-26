PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_space_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`identity_id` text,
	`device_id` text NOT NULL,
	`endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`platform` text NOT NULL,
	`avatar` text,
	`avatar_options` text,
	`relay_url` text,
	`leader_priority` integer DEFAULT 10,
	`authored_by_did` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`identity_id`) REFERENCES `haex_identities`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`device_id`) REFERENCES `haex_devices`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_space_devices`("id", "space_id", "identity_id", "device_id", "endpoint_id", "name", "platform", "avatar", "avatar_options", "relay_url", "leader_priority", "authored_by_did", "created_at") SELECT "id", "space_id", "identity_id", "device_id", "endpoint_id", "name", "platform", "avatar", "avatar_options", "relay_url", "leader_priority", "authored_by_did", "created_at" FROM `haex_space_devices`;--> statement-breakpoint
DROP TABLE `haex_space_devices`;--> statement-breakpoint
ALTER TABLE `__new_haex_space_devices` RENAME TO `haex_space_devices`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_devices_space_device_unique` ON `haex_space_devices` (`space_id`,`device_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_devices_space_endpoint_unique` ON `haex_space_devices` (`space_id`,`endpoint_id`);