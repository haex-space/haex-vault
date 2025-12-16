ALTER TABLE `haex_bridge_authorized_clients` RENAME TO `haex_external_authorized_clients`;--> statement-breakpoint
CREATE TABLE `haex_external_blocked_clients` (
	`id` text PRIMARY KEY NOT NULL,
	`client_id` text NOT NULL,
	`client_name` text NOT NULL,
	`public_key` text NOT NULL,
	`blocked_at` text DEFAULT (CURRENT_TIMESTAMP),
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_external_blocked_clients_client_id_unique` ON `haex_external_blocked_clients` (`client_id`) WHERE "haex_external_blocked_clients"."haex_tombstone" = 0;--> statement-breakpoint
PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_external_authorized_clients` (
	`id` text PRIMARY KEY NOT NULL,
	`client_id` text NOT NULL,
	`client_name` text NOT NULL,
	`public_key` text NOT NULL,
	`extension_id` text NOT NULL,
	`authorized_at` text DEFAULT (CURRENT_TIMESTAMP),
	`last_seen` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text DEFAULT '{}' NOT NULL,
	`haex_tombstone` integer DEFAULT false NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_external_authorized_clients`("id", "client_id", "client_name", "public_key", "extension_id", "authorized_at", "last_seen", "haex_timestamp", "haex_column_hlcs", "haex_tombstone") SELECT "id", "client_id", "client_name", "public_key", "extension_id", "authorized_at", "last_seen", "haex_timestamp", "haex_column_hlcs", "haex_tombstone" FROM `haex_external_authorized_clients`;--> statement-breakpoint
DROP TABLE `haex_external_authorized_clients`;--> statement-breakpoint
ALTER TABLE `__new_haex_external_authorized_clients` RENAME TO `haex_external_authorized_clients`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_external_authorized_clients_client_id_unique` ON `haex_external_authorized_clients` (`client_id`) WHERE "haex_external_authorized_clients"."haex_tombstone" = 0;