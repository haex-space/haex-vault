PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_local_delivery_key_packages_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`target_did` text NOT NULL,
	`package_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_local_delivery_key_packages_no_sync`("id", "space_id", "target_did", "package_blob", "created_at") SELECT "id", "space_id", "target_did", "package_blob", "created_at" FROM `haex_local_delivery_key_packages_no_sync`;--> statement-breakpoint
DROP TABLE `haex_local_delivery_key_packages_no_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_local_delivery_key_packages_no_sync` RENAME TO `haex_local_delivery_key_packages_no_sync`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE INDEX `haex_local_delivery_key_packages_space_did_idx` ON `haex_local_delivery_key_packages_no_sync` (`space_id`,`target_did`);--> statement-breakpoint
CREATE TABLE `__new_haex_local_delivery_messages_no_sync` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`space_id` text NOT NULL,
	`sender_did` text NOT NULL,
	`message_type` text NOT NULL,
	`message_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_local_delivery_messages_no_sync`("id", "space_id", "sender_did", "message_type", "message_blob", "created_at") SELECT "id", "space_id", "sender_did", "message_type", "message_blob", "created_at" FROM `haex_local_delivery_messages_no_sync`;--> statement-breakpoint
DROP TABLE `haex_local_delivery_messages_no_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_local_delivery_messages_no_sync` RENAME TO `haex_local_delivery_messages_no_sync`;--> statement-breakpoint
CREATE INDEX `haex_local_delivery_messages_space_idx` ON `haex_local_delivery_messages_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `__new_haex_local_delivery_pending_commits_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`message_id` integer NOT NULL,
	`expected_dids` text DEFAULT '[]' NOT NULL,
	`acked_dids` text DEFAULT '[]' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_local_delivery_pending_commits_no_sync`("id", "space_id", "message_id", "expected_dids", "acked_dids", "created_at") SELECT "id", "space_id", "message_id", "expected_dids", "acked_dids", "created_at" FROM `haex_local_delivery_pending_commits_no_sync`;--> statement-breakpoint
DROP TABLE `haex_local_delivery_pending_commits_no_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_local_delivery_pending_commits_no_sync` RENAME TO `haex_local_delivery_pending_commits_no_sync`;--> statement-breakpoint
CREATE INDEX `haex_local_delivery_pending_commits_space_idx` ON `haex_local_delivery_pending_commits_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `__new_haex_local_delivery_welcomes_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`recipient_did` text NOT NULL,
	`welcome_blob` blob NOT NULL,
	`consumed` integer DEFAULT 0,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_local_delivery_welcomes_no_sync`("id", "space_id", "recipient_did", "welcome_blob", "consumed", "created_at") SELECT "id", "space_id", "recipient_did", "welcome_blob", "consumed", "created_at" FROM `haex_local_delivery_welcomes_no_sync`;--> statement-breakpoint
DROP TABLE `haex_local_delivery_welcomes_no_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_local_delivery_welcomes_no_sync` RENAME TO `haex_local_delivery_welcomes_no_sync`;--> statement-breakpoint
CREATE INDEX `haex_local_delivery_welcomes_recipient_idx` ON `haex_local_delivery_welcomes_no_sync` (`space_id`,`recipient_did`);--> statement-breakpoint
CREATE TABLE `__new_haex_peer_shares` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`device_endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`local_path` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_peer_shares`("id", "space_id", "device_endpoint_id", "name", "local_path", "created_at") SELECT "id", "space_id", "device_endpoint_id", "name", "local_path", "created_at" FROM `haex_peer_shares`;--> statement-breakpoint
DROP TABLE `haex_peer_shares`;--> statement-breakpoint
ALTER TABLE `__new_haex_peer_shares` RENAME TO `haex_peer_shares`;--> statement-breakpoint
CREATE TABLE `__new_haex_shared_space_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL,
	`space_id` text NOT NULL,
	`extension_id` text,
	`group_id` text,
	`type` text,
	`label` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_shared_space_sync`("id", "table_name", "row_pks", "space_id", "extension_id", "group_id", "type", "label", "created_at") SELECT "id", "table_name", "row_pks", "space_id", "extension_id", "group_id", "type", "label", "created_at" FROM `haex_shared_space_sync`;--> statement-breakpoint
DROP TABLE `haex_shared_space_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_shared_space_sync` RENAME TO `haex_shared_space_sync`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_shared_space_sync_table_row_space_unique` ON `haex_shared_space_sync` (`table_name`,`row_pks`,`space_id`);--> statement-breakpoint
CREATE TABLE `__new_haex_space_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`identity_id` text,
	`device_endpoint_id` text NOT NULL,
	`device_name` text NOT NULL,
	`avatar` text,
	`avatar_options` text,
	`relay_url` text,
	`leader_priority` integer DEFAULT 10,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`identity_id`) REFERENCES `haex_identities`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
INSERT INTO `__new_haex_space_devices`("id", "space_id", "identity_id", "device_endpoint_id", "device_name", "avatar", "avatar_options", "relay_url", "leader_priority", "created_at") SELECT "id", "space_id", "identity_id", "device_endpoint_id", "device_name", "avatar", "avatar_options", "relay_url", "leader_priority", "created_at" FROM `haex_space_devices`;--> statement-breakpoint
DROP TABLE `haex_space_devices`;--> statement-breakpoint
ALTER TABLE `__new_haex_space_devices` RENAME TO `haex_space_devices`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_devices_space_device_unique` ON `haex_space_devices` (`space_id`,`device_endpoint_id`);--> statement-breakpoint
CREATE TABLE `__new_haex_sync_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`home_server_url` text NOT NULL,
	`space_id` text,
	`sync_key` text,
	`vault_key_salt` text,
	`identity_id` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`priority` integer DEFAULT 0 NOT NULL,
	`last_push_hlc_timestamp` text,
	`last_pull_server_timestamp` text,
	`pending_vault_key_update` integer DEFAULT false NOT NULL,
	`type` text DEFAULT 'home' NOT NULL,
	`home_server_did` text,
	`origin_server_did` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_sync_backends`("id", "name", "home_server_url", "space_id", "sync_key", "vault_key_salt", "identity_id", "enabled", "priority", "last_push_hlc_timestamp", "last_pull_server_timestamp", "pending_vault_key_update", "type", "home_server_did", "origin_server_did", "created_at", "updated_at") SELECT "id", "name", "home_server_url", "space_id", "sync_key", "vault_key_salt", "identity_id", "enabled", "priority", "last_push_hlc_timestamp", "last_pull_server_timestamp", "pending_vault_key_update", "type", "home_server_did", "origin_server_did", "created_at", "updated_at" FROM `haex_sync_backends`;--> statement-breakpoint
DROP TABLE `haex_sync_backends`;--> statement-breakpoint
ALTER TABLE `__new_haex_sync_backends` RENAME TO `haex_sync_backends`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_sync_backends_home_server_url_unique` ON `haex_sync_backends` (`home_server_url`);--> statement-breakpoint
CREATE TABLE `__new_haex_sync_rules` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`device_id` text NOT NULL,
	`source_type` text NOT NULL,
	`source_config` text NOT NULL,
	`target_type` text NOT NULL,
	`target_config` text NOT NULL,
	`direction` text DEFAULT 'one_way' NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`sync_interval_seconds` integer DEFAULT 300 NOT NULL,
	`delete_mode` text DEFAULT 'trash' NOT NULL,
	`last_synced_at` integer,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`device_id`) REFERENCES `haex_devices`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
INSERT INTO `__new_haex_sync_rules`("id", "space_id", "device_id", "source_type", "source_config", "target_type", "target_config", "direction", "enabled", "sync_interval_seconds", "delete_mode", "last_synced_at", "created_at") SELECT "id", "space_id", "device_id", "source_type", "source_config", "target_type", "target_config", "direction", "enabled", "sync_interval_seconds", "delete_mode", "last_synced_at", "created_at" FROM `haex_sync_rules`;--> statement-breakpoint
DROP TABLE `haex_sync_rules`;--> statement-breakpoint
ALTER TABLE `__new_haex_sync_rules` RENAME TO `haex_sync_rules`;