ALTER TABLE `haex_shared_space_sync` RENAME COLUMN "group_id" TO "category";--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` RENAME COLUMN "label" TO "type_label";--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` ADD `category_label` text;--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` ADD `authored_by_did` text DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_shared_space_sync` ADD `row_sig` text DEFAULT '' NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_shared_space_sync_author_row_uniq` ON `haex_shared_space_sync` (`authored_by_did`,`space_id`,`table_name`,`row_pks`);
