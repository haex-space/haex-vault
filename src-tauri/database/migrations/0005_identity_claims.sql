CREATE TABLE IF NOT EXISTS `haex_identity_claims` (
	`id` text PRIMARY KEY NOT NULL,
	`identity_id` text NOT NULL REFERENCES `haex_identities`(`id`) ON DELETE CASCADE,
	`type` text NOT NULL,
	`value` text NOT NULL,
	`verified_at` text,
	`verified_by` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE INDEX IF NOT EXISTS `idx_identity_claims_identity` ON `haex_identity_claims` (`identity_id`);
