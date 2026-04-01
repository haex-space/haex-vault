ALTER TABLE `haex_sync_backends` RENAME COLUMN "server_url" TO "home_server_url";--> statement-breakpoint
DROP INDEX `haex_sync_backends_server_url_unique`;--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD `type` text DEFAULT 'home' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD `home_server_did` text;--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD `origin_server_did` text;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_sync_backends_home_server_url_unique` ON `haex_sync_backends` (`home_server_url`);