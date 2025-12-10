CREATE TABLE `haex_bridge_authorized_clients` (
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
CREATE UNIQUE INDEX `haex_bridge_authorized_clients_client_id_unique` ON `haex_bridge_authorized_clients` (`client_id`) WHERE "haex_bridge_authorized_clients"."haex_tombstone" = 0;