PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_shared_space_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL,
	`space_id` text NOT NULL,
	`extension_public_key` text,
	`extension_name` text,
	`group_id` text,
	`type` text,
	`label` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`extension_public_key`,`extension_name`) REFERENCES `haex_extensions`(`public_key`,`name`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "haex_shared_space_sync_extension_pair" CHECK((extension_public_key IS NULL) = (extension_name IS NULL))
);
--> statement-breakpoint
INSERT INTO `__new_haex_shared_space_sync`("id", "table_name", "row_pks", "space_id", "extension_public_key", "extension_name", "group_id", "type", "label", "created_at") SELECT "id", "table_name", "row_pks", "space_id", "extension_public_key", "extension_name", "group_id", "type", "label", "created_at" FROM `haex_shared_space_sync`;--> statement-breakpoint
DROP TABLE `haex_shared_space_sync`;--> statement-breakpoint
ALTER TABLE `__new_haex_shared_space_sync` RENAME TO `haex_shared_space_sync`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_shared_space_sync_table_row_space_unique` ON `haex_shared_space_sync` (`table_name`,`row_pks`,`space_id`);