CREATE TABLE IF NOT EXISTS `haex_identities` (
	`id` text PRIMARY KEY NOT NULL,
	`label` text NOT NULL,
	`did` text NOT NULL,
	`public_key` text NOT NULL,
	`private_key` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS `haex_identities_did_unique` ON `haex_identities` (`did`);
--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD COLUMN `type` text DEFAULT 'personal' NOT NULL;
--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD COLUMN `space_id` text;
--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD COLUMN `space_token` text;
--> statement-breakpoint
ALTER TABLE `haex_sync_backends` ADD COLUMN `identity_id` text;
