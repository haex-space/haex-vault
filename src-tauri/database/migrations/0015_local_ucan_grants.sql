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
CREATE INDEX `haex_space_ucan_grants_active_lookup` ON `haex_space_ucan_grants_no_sync` (`space_id`,`audience_did`) WHERE `revoked_at` IS NULL;
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_ucan_grants_active_uniq` ON `haex_space_ucan_grants_no_sync` (`space_id`,`issuer_did`,`audience_did`,`role`) WHERE `revoked_at` IS NULL;
