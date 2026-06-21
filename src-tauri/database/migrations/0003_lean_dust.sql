DROP TABLE `haex_crdt_pending_columns_no_sync`;
--> statement-breakpoint
CREATE TABLE `haex_crdt_pending_columns_no_sync` (
	`table_name` text NOT NULL,
	`column_name` text NOT NULL,
	`row_pks` text NOT NULL,
	PRIMARY KEY(`table_name`, `column_name`, `row_pks`)
);
