CREATE TABLE `haex_dos_defence_state_no_sync` (
	`id` integer PRIMARY KEY NOT NULL,
	`flood_mode` text DEFAULT 'quiet' NOT NULL,
	`flood_mode_source` text,
	`ddos_expires_at` text,
	`updated_at` text NOT NULL,
	CONSTRAINT "haex_dos_defence_state_singleton" CHECK("haex_dos_defence_state_no_sync"."id" = 1)
);
