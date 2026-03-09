CREATE TABLE IF NOT EXISTS `haex_shared_space_sync` (
	`table_name` text NOT NULL,
	`row_pks` text NOT NULL,
	`space_id` text NOT NULL,
	PRIMARY KEY(`table_name`, `row_pks`, `space_id`)
);
