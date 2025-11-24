CREATE TABLE `haex_crdt_configs` (
	`key` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`value` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_crdt_conflicts` (
	`id` text PRIMARY KEY NOT NULL,
	`table_name` text NOT NULL,
	`conflict_type` text NOT NULL,
	`local_row_id` text NOT NULL,
	`remote_row_id` text NOT NULL,
	`local_row_data` text NOT NULL,
	`remote_row_data` text NOT NULL,
	`local_timestamp` text NOT NULL,
	`remote_timestamp` text NOT NULL,
	`conflict_key` text NOT NULL,
	`detected_at` text NOT NULL,
	`resolved` integer DEFAULT false NOT NULL,
	`resolution` text,
	`resolved_at` text
);
--> statement-breakpoint
CREATE INDEX `haex_crdt_conflicts_table_name_idx` ON `haex_crdt_conflicts` (`table_name`);--> statement-breakpoint
CREATE INDEX `haex_crdt_conflicts_resolved_idx` ON `haex_crdt_conflicts` (`resolved`);--> statement-breakpoint
CREATE TABLE `haex_crdt_dirty_tables` (
	`table_name` text PRIMARY KEY NOT NULL,
	`last_modified` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_desktop_items` (
	`id` text PRIMARY KEY NOT NULL,
	`workspace_id` text NOT NULL,
	`item_type` text NOT NULL,
	`extension_id` text,
	`system_window_id` text,
	`position_x` integer DEFAULT 0 NOT NULL,
	`position_y` integer DEFAULT 0 NOT NULL,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`workspace_id`) REFERENCES `haex_workspaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "item_reference" CHECK(("haex_desktop_items"."item_type" = 'extension' AND "haex_desktop_items"."extension_id" IS NOT NULL AND "haex_desktop_items"."system_window_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'system' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'file' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'folder' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL))
);
--> statement-breakpoint
CREATE TABLE `haex_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`name` text NOT NULL,
	`current` integer DEFAULT false NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_devices_device_id_unique` ON `haex_devices` (`device_id`) WHERE "haex_devices"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_extension_migrations` (
	`id` text PRIMARY KEY NOT NULL,
	`extension_id` text NOT NULL,
	`extension_version` text NOT NULL,
	`migration_name` text NOT NULL,
	`sql_statement` text NOT NULL,
	`applied_at` text DEFAULT (CURRENT_TIMESTAMP),
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extension_migrations_extension_id_migration_name_unique` ON `haex_extension_migrations` (`extension_id`,`migration_name`) WHERE "haex_extension_migrations"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_extension_permissions` (
	`id` text PRIMARY KEY NOT NULL,
	`extension_id` text NOT NULL,
	`resource_type` text,
	`action` text,
	`target` text,
	`constraints` text,
	`status` text DEFAULT 'denied' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extension_permissions_extension_id_resource_type_action_target_unique` ON `haex_extension_permissions` (`extension_id`,`resource_type`,`action`,`target`) WHERE "haex_extension_permissions"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_extensions` (
	`id` text PRIMARY KEY NOT NULL,
	`public_key` text NOT NULL,
	`name` text NOT NULL,
	`version` text NOT NULL,
	`author` text,
	`description` text,
	`entry` text DEFAULT 'index.html',
	`homepage` text,
	`enabled` integer DEFAULT true,
	`icon` text,
	`signature` text NOT NULL,
	`single_instance` integer DEFAULT false,
	`display_mode` text DEFAULT 'auto',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extensions_public_key_name_unique` ON `haex_extensions` (`public_key`,`name`) WHERE "haex_extensions"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_notifications` (
	`id` text PRIMARY KEY NOT NULL,
	`alt` text,
	`date` text,
	`icon` text,
	`image` text,
	`read` integer,
	`source` text,
	`text` text,
	`title` text,
	`type` text NOT NULL,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_sync_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`server_url` text NOT NULL,
	`vault_id` text,
	`email` text,
	`password` text,
	`sync_key` text,
	`enabled` integer DEFAULT true NOT NULL,
	`priority` integer DEFAULT 0 NOT NULL,
	`last_push_hlc_timestamp` text,
	`last_pull_hlc_timestamp` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_sync_backends_server_url_email_unique` ON `haex_sync_backends` (`server_url`,`email`) WHERE "haex_sync_backends"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_vault_settings` (
	`id` text PRIMARY KEY NOT NULL,
	`key` text NOT NULL,
	`type` text NOT NULL,
	`value` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_vault_settings_key_type_unique` ON `haex_vault_settings` (`key`,`type`) WHERE "haex_vault_settings"."haex_tombstone" = 0;--> statement-breakpoint
CREATE TABLE `haex_workspaces` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`name` text NOT NULL,
	`position` integer DEFAULT 0 NOT NULL,
	`background` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_workspaces_position_unique` ON `haex_workspaces` (`position`) WHERE "haex_workspaces"."haex_tombstone" = 0;