CREATE TABLE `haex_bookmark_collections` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_bookmark_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`collection_id` text NOT NULL,
	`replica_id` text NOT NULL,
	`device_label` text NOT NULL,
	`browser_family` text NOT NULL,
	`last_seen_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`collection_id`) REFERENCES `haex_bookmark_collections`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_bookmark_devices_collection_replica_unique` ON `haex_bookmark_devices` (`collection_id`,`replica_id`);--> statement-breakpoint
CREATE TABLE `haex_bookmarks` (
	`id` text PRIMARY KEY NOT NULL,
	`collection_id` text NOT NULL,
	`parent_id` text,
	`root_kind` text,
	`kind` text NOT NULL,
	`title` text,
	`url` text,
	`position` integer NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`collection_id`) REFERENCES `haex_bookmark_collections`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`parent_id`) REFERENCES `haex_bookmarks`(`id`) ON UPDATE no action ON DELETE cascade
);
