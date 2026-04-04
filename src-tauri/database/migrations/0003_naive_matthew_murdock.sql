PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_pending_invites` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`space_name` text,
	`space_type` text,
	`origin_url` text,
	`inviter_did` text NOT NULL,
	`inviter_label` text,
	`capabilities` text,
	`include_history` integer DEFAULT false,
	`token_id` text,
	`space_endpoints` text,
	`status` text DEFAULT 'pending' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`responded_at` text
);
--> statement-breakpoint
INSERT INTO `__new_haex_pending_invites`("id", "space_id", "space_name", "space_type", "origin_url", "inviter_did", "inviter_label", "capabilities", "include_history", "token_id", "space_endpoints", "status", "created_at", "responded_at") SELECT "id", "space_id", "space_name", "space_type", "origin_url", "inviter_did", "inviter_label", "capabilities", "include_history", "token_id", "space_endpoints", "status", "created_at", "responded_at" FROM `haex_pending_invites`;--> statement-breakpoint
DROP TABLE `haex_pending_invites`;--> statement-breakpoint
ALTER TABLE `__new_haex_pending_invites` RENAME TO `haex_pending_invites`;--> statement-breakpoint
PRAGMA foreign_keys=ON;