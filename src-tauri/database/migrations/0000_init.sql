CREATE TABLE `haex_desktop_items_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`workspace_id` text NOT NULL,
	`item_type` text NOT NULL,
	`extension_id` text,
	`system_window_id` text,
	`position_x` integer DEFAULT 0 NOT NULL,
	`position_y` integer DEFAULT 0 NOT NULL,
	FOREIGN KEY (`workspace_id`) REFERENCES `haex_workspaces_no_sync`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "item_reference" CHECK(("haex_desktop_items_no_sync"."item_type" = 'extension' AND "haex_desktop_items_no_sync"."extension_id" IS NOT NULL AND "haex_desktop_items_no_sync"."system_window_id" IS NULL) OR ("haex_desktop_items_no_sync"."item_type" = 'system' AND "haex_desktop_items_no_sync"."system_window_id" IS NOT NULL AND "haex_desktop_items_no_sync"."extension_id" IS NULL) OR ("haex_desktop_items_no_sync"."item_type" = 'file' AND "haex_desktop_items_no_sync"."system_window_id" IS NOT NULL AND "haex_desktop_items_no_sync"."extension_id" IS NULL) OR ("haex_desktop_items_no_sync"."item_type" = 'folder' AND "haex_desktop_items_no_sync"."system_window_id" IS NOT NULL AND "haex_desktop_items_no_sync"."extension_id" IS NULL))
);
--> statement-breakpoint
CREATE TABLE `haex_extension_limits` (
	`id` text PRIMARY KEY NOT NULL,
	`extension_id` text NOT NULL,
	`query_timeout_ms` integer DEFAULT 30000 NOT NULL,
	`max_result_rows` integer DEFAULT 10000 NOT NULL,
	`max_concurrent_queries` integer DEFAULT 5 NOT NULL,
	`max_query_size_bytes` integer DEFAULT 1048576 NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extension_limits_extension_id_unique` ON `haex_extension_limits` (`extension_id`);--> statement-breakpoint
CREATE TABLE `haex_extension_migrations_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`extension_id` text NOT NULL,
	`extension_version` text NOT NULL,
	`migration_name` text NOT NULL,
	`sql_statement` text NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extension_migrations_extension_id_migration_name_unique` ON `haex_extension_migrations_no_sync` (`extension_id`,`migration_name`);--> statement-breakpoint
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
	`i18n` text,
	`dev_path` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_extensions_public_key_name_unique` ON `haex_extensions` (`public_key`,`name`);--> statement-breakpoint
CREATE TABLE `haex_external_authorized_clients_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`client_id` text NOT NULL,
	`client_name` text NOT NULL,
	`public_key` text NOT NULL,
	`extension_id` text NOT NULL,
	`authorized_at` text DEFAULT (CURRENT_TIMESTAMP),
	`last_seen` text,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_external_authorized_clients_client_extension_unique` ON `haex_external_authorized_clients_no_sync` (`client_id`,`extension_id`);--> statement-breakpoint
CREATE TABLE `haex_external_blocked_clients_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`client_id` text NOT NULL,
	`client_name` text NOT NULL,
	`public_key` text NOT NULL,
	`blocked_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_external_blocked_clients_client_id_unique` ON `haex_external_blocked_clients_no_sync` (`client_id`);--> statement-breakpoint
CREATE TABLE `haex_logs` (
	`id` text PRIMARY KEY NOT NULL,
	`timestamp` text NOT NULL,
	`level` text NOT NULL,
	`source` text NOT NULL,
	`extension_id` text,
	`message` text NOT NULL,
	`metadata` text,
	`device_id` text NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
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
	`type` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_vault_settings` (
	`id` text PRIMARY KEY NOT NULL,
	`key` text NOT NULL,
	`value` text,
	`device_id` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_vault_settings_key_device_unique` ON `haex_vault_settings` (`key`,`device_id`);--> statement-breakpoint
CREATE TABLE `haex_workspaces_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`device_id` text NOT NULL,
	`name` text NOT NULL,
	`position` integer DEFAULT 0 NOT NULL,
	`background` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_workspaces_device_position_unique` ON `haex_workspaces_no_sync` (`device_id`,`position`);--> statement-breakpoint
CREATE TABLE `haex_crdt_configs_no_sync` (
	`key` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`value` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_crdt_conflicts_no_sync` (
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
CREATE INDEX `haex_crdt_conflicts_no_sync_table_name_idx` ON `haex_crdt_conflicts_no_sync` (`table_name`);--> statement-breakpoint
CREATE INDEX `haex_crdt_conflicts_no_sync_resolved_idx` ON `haex_crdt_conflicts_no_sync` (`resolved`);--> statement-breakpoint
CREATE TABLE `haex_crdt_dirty_tables_no_sync` (
	`table_name` text PRIMARY KEY NOT NULL,
	`last_modified` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_crdt_migrations_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`extension_id` text,
	`migration_name` text NOT NULL,
	`migration_content` text NOT NULL,
	`applied_at` text NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_crdt_migrations_no_sync_ext_name_unique` ON `haex_crdt_migrations_no_sync` (`extension_id`,`migration_name`);--> statement-breakpoint
CREATE TABLE `haex_crdt_pending_columns_no_sync` (
	`table_name` text NOT NULL,
	`column_name` text NOT NULL,
	PRIMARY KEY(`table_name`, `column_name`)
);
--> statement-breakpoint
CREATE TABLE `haex_identities` (
	`id` text PRIMARY KEY NOT NULL,
	`public_key` text NOT NULL,
	`did` text NOT NULL,
	`label` text NOT NULL,
	`private_key` text,
	`avatar` text,
	`notes` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_identities_public_key_unique` ON `haex_identities` (`public_key`);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_identities_did_unique` ON `haex_identities` (`did`);--> statement-breakpoint
CREATE TABLE `haex_identity_claims` (
	`id` text PRIMARY KEY NOT NULL,
	`identity_id` text NOT NULL,
	`type` text NOT NULL,
	`value` text NOT NULL,
	`verified_at` text,
	`verified_by` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`identity_id`) REFERENCES `haex_identities`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_blocked_dids` (
	`id` text PRIMARY KEY NOT NULL,
	`did` text NOT NULL,
	`label` text,
	`blocked_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_blocked_dids_did_unique` ON `haex_blocked_dids` (`did`);--> statement-breakpoint
CREATE TABLE `haex_invite_outbox` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`token_id` text NOT NULL,
	`target_did` text NOT NULL,
	`target_endpoint_id` text NOT NULL,
	`status` text DEFAULT 'pending' NOT NULL,
	`retry_count` integer DEFAULT 0 NOT NULL,
	`next_retry_at` text DEFAULT (CURRENT_TIMESTAMP),
	`expires_at` text DEFAULT '',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_invite_policy` (
	`id` text PRIMARY KEY NOT NULL,
	`policy` text DEFAULT 'all' NOT NULL,
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_invite_tokens` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`target_did` text,
	`capabilities` text,
	`pre_created_ucan` text,
	`include_history` integer DEFAULT false,
	`max_uses` integer DEFAULT 1 NOT NULL,
	`current_uses` integer DEFAULT 0 NOT NULL,
	`expires_at` text DEFAULT '',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_pending_invites` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`inviter_did` text NOT NULL,
	`inviter_label` text,
	`capabilities` text,
	`include_history` integer DEFAULT false,
	`token_id` text,
	`space_endpoints` text,
	`status` text DEFAULT 'pending' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`responded_at` text,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_local_delivery_key_packages_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`target_did` text NOT NULL,
	`package_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_delivery_key_packages_space_did_idx` ON `haex_local_delivery_key_packages_no_sync` (`space_id`,`target_did`);--> statement-breakpoint
CREATE TABLE `haex_local_delivery_messages_no_sync` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`space_id` text NOT NULL,
	`sender_did` text NOT NULL,
	`message_type` text NOT NULL,
	`message_blob` blob NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_delivery_messages_space_idx` ON `haex_local_delivery_messages_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `haex_local_delivery_pending_commits_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`commit_blob` blob NOT NULL,
	`delivered_to` text DEFAULT '[]',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_delivery_pending_commits_space_idx` ON `haex_local_delivery_pending_commits_no_sync` (`space_id`);--> statement-breakpoint
CREATE TABLE `haex_local_delivery_welcomes_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`recipient_did` text NOT NULL,
	`welcome_blob` blob NOT NULL,
	`consumed` integer DEFAULT 0,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_delivery_welcomes_recipient_idx` ON `haex_local_delivery_welcomes_no_sync` (`space_id`,`recipient_did`);--> statement-breakpoint
CREATE TABLE `haex_mls_epoch_key_pairs_no_sync` (
	`group_id` blob NOT NULL,
	`epoch_bytes` blob NOT NULL,
	`leaf_index` integer NOT NULL,
	`value_blob` blob NOT NULL,
	PRIMARY KEY(`group_id`, `epoch_bytes`, `leaf_index`)
);
--> statement-breakpoint
CREATE TABLE `haex_mls_list_no_sync` (
	`store_type` text NOT NULL,
	`key_bytes` blob NOT NULL,
	`index_num` integer NOT NULL,
	`value_blob` blob NOT NULL,
	PRIMARY KEY(`store_type`, `key_bytes`, `index_num`)
);
--> statement-breakpoint
CREATE TABLE `haex_mls_values_no_sync` (
	`store_type` text NOT NULL,
	`key_bytes` blob NOT NULL,
	`value_blob` blob NOT NULL,
	PRIMARY KEY(`store_type`, `key_bytes`)
);
--> statement-breakpoint
CREATE TABLE `haex_device_mls_enrollments` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`device_id` text NOT NULL,
	`key_package` text NOT NULL,
	`welcome` text,
	`status` text DEFAULT 'pending' NOT NULL,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_mls_sync_keys` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`epoch` integer NOT NULL,
	`key_data` text NOT NULL,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_peer_shares` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`device_endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`local_path` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE TABLE `haex_shared_space_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL,
	`space_id` text NOT NULL,
	`extension_id` text,
	`group_id` text,
	`type` text,
	`label` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_shared_space_sync_table_row_space_unique` ON `haex_shared_space_sync` (`table_name`,`row_pks`,`space_id`);--> statement-breakpoint
CREATE TABLE `haex_space_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`identity_id` text,
	`device_endpoint_id` text NOT NULL,
	`device_name` text NOT NULL,
	`avatar` text,
	`relay_url` text,
	`leader_priority` integer DEFAULT 10,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`identity_id`) REFERENCES `haex_identities`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_devices_space_device_unique` ON `haex_space_devices` (`space_id`,`device_endpoint_id`);--> statement-breakpoint
CREATE TABLE `haex_spaces` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text DEFAULT 'online' NOT NULL,
	`status` text DEFAULT 'active' NOT NULL,
	`name` text NOT NULL,
	`origin_url` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`modified_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_sync_backends` (
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
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_sync_backends_home_server_url_unique` ON `haex_sync_backends` (`home_server_url`);--> statement-breakpoint
CREATE TABLE `haex_ucan_tokens` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`token` text NOT NULL,
	`capability` text NOT NULL,
	`issuer_did` text NOT NULL,
	`audience_did` text NOT NULL,
	`issued_at` integer NOT NULL,
	`expires_at` integer NOT NULL,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_storage_backends` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`name` text NOT NULL,
	`config` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_storage_backends_name_unique` ON `haex_storage_backends` (`name`);