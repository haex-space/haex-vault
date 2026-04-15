ALTER TABLE `haex_identities` RENAME COLUMN "label" TO "name";--> statement-breakpoint
DROP INDEX `haex_identities_public_key_unique`;--> statement-breakpoint
ALTER TABLE `haex_identities` ADD `source` text DEFAULT 'contact' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_identities` DROP COLUMN `public_key`;--> statement-breakpoint
DROP INDEX `haex_space_members_space_did_unique`;--> statement-breakpoint
ALTER TABLE `haex_space_members` ADD `identity_id` text NOT NULL REFERENCES haex_identities(id);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_members_space_identity_unique` ON `haex_space_members` (`space_id`,`identity_id`);--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `member_did`;--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `member_public_key`;--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `label`;--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `avatar`;--> statement-breakpoint
ALTER TABLE `haex_space_members` DROP COLUMN `avatar_options`;--> statement-breakpoint
ALTER TABLE `haex_spaces` ADD `owner_identity_id` text NOT NULL REFERENCES haex_identities(id);