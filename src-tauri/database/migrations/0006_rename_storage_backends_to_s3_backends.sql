ALTER TABLE `haex_storage_backends` RENAME TO `haex_s3_backends`;--> statement-breakpoint
DROP INDEX `haex_storage_backends_name_unique`;--> statement-breakpoint
ALTER TABLE `haex_s3_backends` ADD `parent_backend_id` text REFERENCES haex_s3_backends(id);--> statement-breakpoint
ALTER TABLE `haex_s3_backends` ADD `origin_type` text DEFAULT 'owned' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_s3_backends` ADD `share_prefix` text;--> statement-breakpoint
ALTER TABLE `haex_s3_backends` ADD `share_access_flags` integer;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_s3_backends_name_unique` ON `haex_s3_backends` (`name`);