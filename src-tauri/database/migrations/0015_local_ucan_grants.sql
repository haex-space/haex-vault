CREATE TABLE `haex_space_ucan_grants_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`issuer_did` text NOT NULL,
	`audience_did` text NOT NULL,
	`ucan_token` text NOT NULL,
	`role` text NOT NULL,
	`created_at` text NOT NULL,
	`revoked_at` text,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "haex_space_ucan_grants_no_sync_role_check" CHECK(role IN ('issued','received'))
);
--> statement-breakpoint
CREATE INDEX `haex_space_ucan_grants_lookup` ON `haex_space_ucan_grants_no_sync` (`space_id`,`audience_did`,`revoked_at`);
