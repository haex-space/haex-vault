DROP TABLE IF EXISTS `haex_local_delivery_pending_commits_no_sync`;
--> statement-breakpoint
CREATE TABLE `haex_local_delivery_pending_commits_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`message_id` integer NOT NULL,
	`expected_dids` text DEFAULT '[]' NOT NULL,
	`acked_dids` text DEFAULT '[]' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `haex_local_delivery_pending_commits_space_idx` ON `haex_local_delivery_pending_commits_no_sync` (`space_id`);
