CREATE TABLE `haex_storage_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`name` text NOT NULL,
	`config` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_storage_backends_name_unique` ON `haex_storage_backends` (`name`) WHERE "haex_storage_backends"."haex_tombstone" = 0;