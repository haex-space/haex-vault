CREATE TABLE IF NOT EXISTS `haex_space_keys_no_sync` (
	`space_id` text NOT NULL,
	`generation` integer NOT NULL,
	`key` text NOT NULL,
	PRIMARY KEY(`space_id`, `generation`)
);
