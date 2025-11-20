ALTER TABLE `haex_crdt_changes` ADD `device_id` text;--> statement-breakpoint
CREATE INDEX `idx_crdt_changes_device_id` ON `haex_crdt_changes` (`device_id`);