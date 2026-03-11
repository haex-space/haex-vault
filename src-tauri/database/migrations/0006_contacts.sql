CREATE TABLE IF NOT EXISTS `haex_contacts` (
	`id` text PRIMARY KEY NOT NULL,
	`label` text NOT NULL,
	`public_key` text NOT NULL,
	`notes` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS `haex_contacts_public_key_unique` ON `haex_contacts` (`public_key`);
--> statement-breakpoint
CREATE TABLE IF NOT EXISTS `haex_contact_claims` (
	`id` text PRIMARY KEY NOT NULL,
	`contact_id` text NOT NULL REFERENCES `haex_contacts`(`id`) ON DELETE CASCADE,
	`type` text NOT NULL,
	`value` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
