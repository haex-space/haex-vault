DROP INDEX `haex_deleted_rows_table_row_pks_unique`;--> statement-breakpoint
CREATE INDEX `haex_deleted_rows_table_row_pks_idx` ON `haex_deleted_rows` (`table_name`,`row_pks`);