CREATE TABLE `haex_file_sync_queue` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`rule_id` text NOT NULL,
	`local_path` text NOT NULL,
	`relative_path` text NOT NULL,
	`operation` text NOT NULL,
	`status` text DEFAULT 'pending' NOT NULL,
	`priority` integer DEFAULT 100 NOT NULL,
	`file_size` integer DEFAULT 0 NOT NULL,
	`error_message` text,
	`retry_count` integer DEFAULT 0 NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`started_at` text,
	`completed_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`rule_id`) REFERENCES `haex_file_sync_rules`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_sync_queue_device_rule_path_unique` ON `haex_file_sync_queue` (`device_id`,`rule_id`,`local_path`) WHERE "haex_file_sync_queue"."haex_tombstone" = 0 AND "haex_file_sync_queue"."status" IN ('pending', 'in_progress');--> statement-breakpoint
CREATE INDEX `haex_file_sync_queue_device_status_idx` ON `haex_file_sync_queue` (`device_id`,`status`);--> statement-breakpoint
CREATE INDEX `haex_file_sync_queue_priority_idx` ON `haex_file_sync_queue` (`priority`);