CREATE TABLE `haex_crdt_changes` (
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL,
	`column_name` text,
	`operation` text NOT NULL,
	`hlc_timestamp` text NOT NULL,
	`sync_state` text DEFAULT 'pending_upload' NOT NULL,
	`created_at` text NOT NULL,
	PRIMARY KEY(`table_name`, `row_pks`, `column_name`)
);
--> statement-breakpoint
CREATE INDEX `idx_crdt_changes_sync_state` ON `haex_crdt_changes` (`sync_state`);--> statement-breakpoint
CREATE INDEX `idx_crdt_changes_hlc` ON `haex_crdt_changes` (`hlc_timestamp`);--> statement-breakpoint
CREATE INDEX `idx_crdt_changes_table_row` ON `haex_crdt_changes` (`table_name`,`row_pks`);--> statement-breakpoint
CREATE TABLE `haex_crdt_configs` (
	`key` text PRIMARY KEY NOT NULL,
	`value` text
);
--> statement-breakpoint
CREATE TABLE `haex_crdt_snapshots` (
	`snapshot_id` text PRIMARY KEY NOT NULL,
	`created` text,
	`epoch_hlc` text,
	`location_url` text,
	`file_size_bytes` integer
);
--> statement-breakpoint
CREATE TABLE `haex_crdt_sync_status` (
	`id` text PRIMARY KEY NOT NULL,
	`backend_id` text NOT NULL,
	`last_pull_created_at` text,
	`last_push_hlc_timestamp` text,
	`last_sync_at` text,
	`error` text
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
	FOREIGN KEY (`workspace_id`) REFERENCES `haex_workspaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "item_reference" CHECK(("haex_desktop_items"."item_type" = 'extension' AND "haex_desktop_items"."extension_id" IS NOT NULL AND "haex_desktop_items"."system_window_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'system' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'file' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL) OR ("haex_desktop_items"."item_type" = 'folder' AND "haex_desktop_items"."system_window_id" IS NOT NULL AND "haex_desktop_items"."extension_id" IS NULL))
);
--> statement-breakpoint
CREATE TABLE `haex_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`name` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_devices_device_id_unique` ON `haex_devices` (`device_id`);--> statement-breakpoint
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
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extension_permissions_extension_id_resource_type_action_target_unique` ON `haex_extension_permissions` (`extension_id`,`resource_type`,`action`,`target`);--> statement-breakpoint
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
	`haex_timestamp` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extensions_public_key_name_unique` ON `haex_extensions` (`public_key`,`name`);--> statement-breakpoint
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
	`haex_timestamp` text
);
--> statement-breakpoint
CREATE TABLE `haex_settings` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text,
	`key` text,
	`type` text,
	`value` text,
	`haex_timestamp` text,
	FOREIGN KEY (`device_id`) REFERENCES `haex_devices`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_settings_device_id_key_type_unique` ON `haex_settings` (`device_id`,`key`,`type`);--> statement-breakpoint
CREATE TABLE `haex_sync_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`server_url` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`priority` integer DEFAULT 0 NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	`haex_timestamp` text
);
--> statement-breakpoint
CREATE TABLE `haex_workspaces` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`name` text NOT NULL,
	`position` integer DEFAULT 0 NOT NULL,
	`background` text,
	`haex_timestamp` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_workspaces_position_unique` ON `haex_workspaces` (`position`);