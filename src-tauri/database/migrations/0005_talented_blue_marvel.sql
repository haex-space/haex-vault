CREATE TABLE `haex_passwords_binaries` (
	`hash` text PRIMARY KEY NOT NULL,
	`data` text NOT NULL,
	`size` integer NOT NULL,
	`type` text DEFAULT 'attachment',
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_generator_presets` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`length` integer DEFAULT 16 NOT NULL,
	`uppercase` integer DEFAULT true NOT NULL,
	`lowercase` integer DEFAULT true NOT NULL,
	`numbers` integer DEFAULT true NOT NULL,
	`symbols` integer DEFAULT true NOT NULL,
	`exclude_chars` text DEFAULT '',
	`use_pattern` integer DEFAULT false NOT NULL,
	`pattern` text DEFAULT '',
	`is_default` integer DEFAULT false NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_group_items` (
	`item_id` text PRIMARY KEY NOT NULL,
	`group_id` text,
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`group_id`) REFERENCES `haex_passwords_groups`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_groups` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text,
	`description` text,
	`icon` text,
	`sort_order` integer,
	`color` text,
	`parent_id` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`parent_id`) REFERENCES `haex_passwords_groups`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_item_binaries` (
	`id` text PRIMARY KEY NOT NULL,
	`item_id` text NOT NULL,
	`binary_hash` text NOT NULL,
	`file_name` text NOT NULL,
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`binary_hash`) REFERENCES `haex_passwords_binaries`(`hash`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_item_details` (
	`id` text PRIMARY KEY NOT NULL,
	`title` text,
	`username` text,
	`password` text,
	`note` text,
	`icon` text,
	`color` text,
	`url` text,
	`otp_secret` text,
	`otp_digits` integer DEFAULT 6,
	`otp_period` integer DEFAULT 30,
	`otp_algorithm` text DEFAULT 'SHA1',
	`expires_at` text,
	`autofill_aliases` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_item_key_values` (
	`id` text PRIMARY KEY NOT NULL,
	`item_id` text NOT NULL,
	`key` text,
	`value` text,
	`updated_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_item_snapshots` (
	`id` text PRIMARY KEY NOT NULL,
	`item_id` text NOT NULL,
	`snapshot_data` text NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`modified_at` text,
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_item_tags` (
	`id` text PRIMARY KEY NOT NULL,
	`item_id` text NOT NULL,
	`tag_id` text NOT NULL,
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`tag_id`) REFERENCES `haex_passwords_tags`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_passwords_item_tags_item_tag_unique` ON `haex_passwords_item_tags` (`item_id`,`tag_id`);--> statement-breakpoint
CREATE TABLE `haex_passwords_passkeys` (
	`id` text PRIMARY KEY NOT NULL,
	`item_id` text,
	`credential_id` text NOT NULL,
	`relying_party_id` text NOT NULL,
	`relying_party_name` text,
	`user_handle` text NOT NULL,
	`user_name` text,
	`user_display_name` text,
	`private_key` text NOT NULL,
	`public_key` text NOT NULL,
	`algorithm` integer DEFAULT -7 NOT NULL,
	`sign_count` integer DEFAULT 0 NOT NULL,
	`is_discoverable` integer DEFAULT true NOT NULL,
	`icon` text,
	`color` text,
	`nickname` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`last_used_at` text,
	FOREIGN KEY (`item_id`) REFERENCES `haex_passwords_item_details`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_passwords_passkeys_credential_id_unique` ON `haex_passwords_passkeys` (`credential_id`);--> statement-breakpoint
CREATE TABLE `haex_passwords_snapshot_binaries` (
	`id` text PRIMARY KEY NOT NULL,
	`snapshot_id` text NOT NULL,
	`binary_hash` text NOT NULL,
	`file_name` text NOT NULL,
	FOREIGN KEY (`snapshot_id`) REFERENCES `haex_passwords_item_snapshots`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`binary_hash`) REFERENCES `haex_passwords_binaries`(`hash`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `haex_passwords_tags` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`color` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_passwords_tags_name_unique` ON `haex_passwords_tags` (`name`);