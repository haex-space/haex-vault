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
CREATE INDEX `haex_crdt_conflicts_resolved_idx` ON `haex_crdt_conflicts` (`resolved`);