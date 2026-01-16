-- Add dev_path column to haex_extensions table
-- If dev_path is set, this is a dev extension; if NULL, this is a production extension
ALTER TABLE `haex_extensions` ADD `dev_path` text;
--> statement-breakpoint
-- Create pending columns table for tracking skipped columns during sync
-- When a remote device has a newer schema version with additional columns,
-- we skip those columns but track them here. After the app updates and migrations
-- add the missing columns, we pull ALL data for these columns from the server.
-- Only stores (table_name, column_name) - the actual row PKs come from the server
-- during the re-pull after migration.
CREATE TABLE `haex_crdt_pending_columns_no_sync` (
	`table_name` text NOT NULL,
	`column_name` text NOT NULL,
	PRIMARY KEY(`table_name`, `column_name`)
);
