CREATE TABLE `haex_sync_rules` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`name` text NOT NULL,
	`source_type` text NOT NULL,
	`source_config` text NOT NULL,
	`target_type` text NOT NULL,
	`target_config` text NOT NULL,
	`direction` text DEFAULT 'one_way' NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`sync_interval_seconds` integer DEFAULT 300 NOT NULL,
	`delete_mode` text DEFAULT 'trash' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE TABLE `haex_sync_state_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`rule_id` text NOT NULL,
	`relative_path` text NOT NULL,
	`file_size` integer NOT NULL,
	`modified_at` integer NOT NULL,
	`synced_at` text NOT NULL,
	`deleted` integer DEFAULT false NOT NULL
);
