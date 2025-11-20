ALTER TABLE `haex_sync_backends` ADD `email` text;--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD `password` text;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_sync_backends_server_url_email_unique` ON `haex_sync_backends` (`server_url`,`email`);