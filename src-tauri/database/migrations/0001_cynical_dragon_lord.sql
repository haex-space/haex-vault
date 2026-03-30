CREATE TABLE `haex_local_ds_key_packages_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`target_did` text NOT NULL,
	`package_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_ds_key_packages_space_did_idx` ON `haex_local_ds_key_packages_no_sync` (`space_id`,`target_did`);--> statement-breakpoint
CREATE TABLE `haex_local_ds_messages_no_sync` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`space_id` text NOT NULL,
	`sender_did` text NOT NULL,
	`message_type` text NOT NULL,
	`message_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_ds_messages_space_idx` ON `haex_local_ds_messages_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `haex_local_ds_pending_commits_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`commit_blob` blob NOT NULL,
	`delivered_to` text DEFAULT '[]',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_ds_pending_commits_space_idx` ON `haex_local_ds_pending_commits_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `haex_local_ds_welcomes_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`recipient_did` text NOT NULL,
	`welcome_blob` blob NOT NULL,
	`consumed` integer DEFAULT 0,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_ds_welcomes_recipient_idx` ON `haex_local_ds_welcomes_no_sync` (`space_id`,`recipient_did`);--> statement-breakpoint
ALTER TABLE `haex_space_devices` ADD `leader_priority` integer DEFAULT 10;