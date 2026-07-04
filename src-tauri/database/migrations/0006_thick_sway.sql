ALTER TABLE `haex_storage_backends` ADD `parent_backend_id` text REFERENCES haex_storage_backends(id);--> statement-breakpoint
ALTER TABLE `haex_storage_backends` ADD `origin_type` text DEFAULT 'owned' NOT NULL;--> statement-breakpoint
ALTER TABLE `haex_storage_backends` ADD `share_prefix` text;--> statement-breakpoint
ALTER TABLE `haex_storage_backends` ADD `share_access_flags` integer;