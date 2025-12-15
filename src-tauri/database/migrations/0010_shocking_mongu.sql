CREATE TABLE `haex_file_backend_mapping` (
	`id` text PRIMARY KEY NOT NULL,
	`file_id` text NOT NULL,
	`backend_id` text NOT NULL,
	`remote_id` text NOT NULL,
	`uploaded_at` text,
	`verified_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`file_id`) REFERENCES `haex_files`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`backend_id`) REFERENCES `haex_file_backends`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_backend_mapping_file_backend_unique` ON `haex_file_backend_mapping` (`file_id`,`backend_id`) WHERE "haex_file_backend_mapping"."haex_tombstone" = 0;--> statement-breakpoint
CREATE INDEX `haex_file_backend_mapping_file_id_idx` ON `haex_file_backend_mapping` (`file_id`);--> statement-breakpoint
CREATE INDEX `haex_file_backend_mapping_backend_id_idx` ON `haex_file_backend_mapping` (`backend_id`);--> statement-breakpoint
CREATE TABLE `haex_file_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`name` text NOT NULL,
	`encrypted_config` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_backends_name_unique` ON `haex_file_backends` (`name`) WHERE "haex_file_backends"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_file_chunks` (
	`id` text PRIMARY KEY NOT NULL,
	`file_id` text NOT NULL,
	`chunk_index` integer NOT NULL,
	`remote_id` text,
	`size` integer NOT NULL,
	`encrypted_hash` text NOT NULL,
	`uploaded` integer DEFAULT false NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`file_id`) REFERENCES `haex_files`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_chunks_file_chunk_unique` ON `haex_file_chunks` (`file_id`,`chunk_index`) WHERE "haex_file_chunks"."haex_tombstone" = 0;--> statement-breakpoint
CREATE INDEX `haex_file_chunks_file_id_idx` ON `haex_file_chunks` (`file_id`);--> statement-breakpoint
CREATE TABLE `haex_file_local_sync_state` (
	`id` text PRIMARY KEY NOT NULL,
	`file_id` text NOT NULL,
	`local_path` text NOT NULL,
	`local_hash` text NOT NULL,
	`local_mtime` text NOT NULL,
	`local_size` integer NOT NULL,
	`synced_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`file_id`) REFERENCES `haex_files`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_local_sync_state_file_id_unique` ON `haex_file_local_sync_state` (`file_id`) WHERE "haex_file_local_sync_state"."haex_tombstone" = 0;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_local_sync_state_local_path_unique` ON `haex_file_local_sync_state` (`local_path`) WHERE "haex_file_local_sync_state"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_file_spaces` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`is_personal` integer DEFAULT true NOT NULL,
	`wrapped_key` text NOT NULL,
	`file_count` integer DEFAULT 0 NOT NULL,
	`total_size` integer DEFAULT 0 NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_spaces_name_unique` ON `haex_file_spaces` (`name`) WHERE "haex_file_spaces"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_file_sync_errors` (
	`id` text PRIMARY KEY NOT NULL,
	`file_id` text,
	`backend_id` text,
	`error_type` text NOT NULL,
	`error_message` text NOT NULL,
	`retry_count` integer DEFAULT 0 NOT NULL,
	`last_retry_at` text,
	`resolved` integer DEFAULT false NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`file_id`) REFERENCES `haex_files`(`id`) ON UPDATE no action ON DELETE set null,
	FOREIGN KEY (`backend_id`) REFERENCES `haex_file_backends`(`id`) ON UPDATE no action ON DELETE set null
);
--> statement-breakpoint
CREATE INDEX `haex_file_sync_errors_resolved_idx` ON `haex_file_sync_errors` (`resolved`);--> statement-breakpoint
CREATE TABLE `haex_file_sync_rule_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`rule_id` text NOT NULL,
	`backend_id` text NOT NULL,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`rule_id`) REFERENCES `haex_file_sync_rules`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`backend_id`) REFERENCES `haex_file_backends`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_sync_rule_backends_rule_backend_unique` ON `haex_file_sync_rule_backends` (`rule_id`,`backend_id`) WHERE "haex_file_sync_rule_backends"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_file_sync_rules` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`local_path` text NOT NULL,
	`direction` text DEFAULT 'both' NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`last_sync_at` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`space_id`) REFERENCES `haex_file_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_sync_rules_local_path_unique` ON `haex_file_sync_rules` (`local_path`) WHERE "haex_file_sync_rules"."haex_tombstone" = 0;--> statement-breakpoint
CREATE INDEX `haex_file_sync_rules_space_id_idx` ON `haex_file_sync_rules` (`space_id`);--> statement-breakpoint
CREATE TABLE `haex_files` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`parent_id` text,
	`encrypted_name` text NOT NULL,
	`encrypted_path` text NOT NULL,
	`encrypted_mime_type` text,
	`is_directory` integer DEFAULT false NOT NULL,
	`size` integer DEFAULT 0 NOT NULL,
	`content_hash` text,
	`wrapped_key` text,
	`chunk_count` integer DEFAULT 0 NOT NULL,
	`sync_state` text DEFAULT 'local_only' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`space_id`) REFERENCES `haex_file_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`parent_id`) REFERENCES `haex_files`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `haex_files_space_id_idx` ON `haex_files` (`space_id`);--> statement-breakpoint
CREATE INDEX `haex_files_parent_id_idx` ON `haex_files` (`parent_id`);--> statement-breakpoint
CREATE INDEX `haex_files_sync_state_idx` ON `haex_files` (`sync_state`);