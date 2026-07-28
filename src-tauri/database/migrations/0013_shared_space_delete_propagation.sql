CREATE TABLE `haex_shared_space_deleted_rows` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL
);
--> statement-breakpoint
CREATE INDEX `idx_haex_shared_space_deleted_rows_space` ON `haex_shared_space_deleted_rows` (`space_id`,`table_name`);--> statement-breakpoint
CREATE TABLE `haex_space_compaction_anchors` (
	`space_id` text PRIMARY KEY NOT NULL,
	`min_valid_hlc` text DEFAULT '0' NOT NULL
);
