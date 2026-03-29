CREATE TABLE `haex_blocked_dids` (
	`id` text PRIMARY KEY NOT NULL,
	`did` text NOT NULL,
	`label` text,
	`blocked_at` text NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_blocked_dids_did_unique` ON `haex_blocked_dids` (`did`);--> statement-breakpoint
CREATE TABLE `haex_invite_policy` (
	`id` text PRIMARY KEY NOT NULL,
	`policy` text DEFAULT 'all' NOT NULL,
	`updated_at` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `haex_pending_invites` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`inviter_did` text NOT NULL,
	`inviter_label` text,
	`space_name` text,
	`status` text DEFAULT 'pending' NOT NULL,
	`include_history` integer DEFAULT false,
	`created_at` text NOT NULL,
	`responded_at` text
);
--> statement-breakpoint
CREATE TABLE `haex_ucan_tokens` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`token` text NOT NULL,
	`capability` text NOT NULL,
	`issuer_did` text NOT NULL,
	`audience_did` text NOT NULL,
	`issued_at` integer NOT NULL,
	`expires_at` integer NOT NULL
);
--> statement-breakpoint
ALTER TABLE `haex_identities` ADD `agreement_public_key` text NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_identities` ADD `agreement_private_key` text NOT NULL;