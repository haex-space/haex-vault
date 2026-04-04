CREATE TABLE `haex_space_members` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`member_did` text NOT NULL,
	`member_public_key` text NOT NULL,
	`label` text NOT NULL,
	`avatar` text,
	`avatar_options` text,
	`role` text DEFAULT 'read' NOT NULL,
	`joined_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_members_space_did_unique` ON `haex_space_members` (`space_id`,`member_did`);