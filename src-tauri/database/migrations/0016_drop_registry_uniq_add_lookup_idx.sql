DROP INDEX IF EXISTS `haex_shared_space_sync_author_category_uniq`;--> statement-breakpoint
DROP INDEX IF EXISTS `haex_shared_space_sync_author_row_uniq`;--> statement-breakpoint
CREATE INDEX `haex_shared_space_sync_author_category_idx` ON `haex_shared_space_sync` (`authored_by_did`,`space_id`,`table_name`,`category`);
